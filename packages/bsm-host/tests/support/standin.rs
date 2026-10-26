//! A minimal stand-in for bsm's hosted-session side: a `wayland-server`
//! display that offers `wl_compositor`, `wl_shm`, `wl_seat` and
//! `bsm_session_host_v1` and records what the bridge does. It implements the
//! protocol's rules (configure, ack, presented after ack) the way the real
//! compositor must, so the bridge can be tested without bsm.
//!
//! It is test support only. It is driven through [`Standin`] from the test
//! thread and observed through [`Shared`].

#![allow(dead_code, clippy::all)]

use std::collections::HashMap;
use std::io::Write;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use memmap2::Mmap;
use wayland_server::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_region, wl_seat, wl_shm, wl_shm_pool,
    wl_surface, wl_touch,
};
use wayland_server::{
    Client, DataInit, Dispatch, Display, DisplayHandle, GlobalDispatch, ListeningSocket, New, Resource, WEnum,
};
use wayland_server::backend::ClientData;

pub mod proto {
    #![allow(non_camel_case_types, unused_imports, clippy::all)]
    use wayland_server;
    use wayland_server::protocol::*;
    pub mod __interfaces {
        use wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("../bsm/protocol/bsm-session-host-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_server_code!("../bsm/protocol/bsm-session-host-v1.xml");
}
use proto::bsm_session_host_v1::{self, BsmSessionHostV1};
use proto::bsm_session_surface_v1::{self, BsmSessionSurfaceV1};

#[derive(Debug, Clone, Copy)]
pub struct OutputSpec {
    pub x: i32,
    pub y: i32,
    /// Logical size; the configure carries `ceil(size * scale_120 / 120)` pixels.
    pub width: u32,
    pub height: u32,
    pub scale_120: u32,
    pub refresh_mhz: u32,
    pub transform: u32,
}

impl OutputSpec {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height, scale_120: 120, refresh_mhz: 60_000, transform: 0 }
    }
    pub fn pixels(&self) -> (u32, u32) {
        (
            (self.width as u64 * self.scale_120 as u64).div_ceil(120) as u32,
            (self.height as u64 * self.scale_120 as u64).div_ceil(120) as u32,
        )
    }
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub output: String,
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    /// XRGB8888 bytes (B, G, R, X).
    pub pixels: Vec<u8>,
    /// The commit came after the ack of the latest configure.
    pub after_ack: bool,
    pub damage: Vec<(i32, i32, i32, i32)>,
    pub at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportRec {
    pub kind: u32,
    pub output: Option<String>,
    pub a: u32,
    pub b: u32,
    pub generation: String,
    pub at: Instant,
}

#[derive(Default)]
pub struct Shared {
    pub frames: Vec<Frame>,
    pub reports: Vec<ReportRec>,
    pub configures: Vec<(String, u32, u32, u32, u32)>,
    pub acks: Vec<(String, u32)>,
    /// Protocol violations by the client (buffer before ack, wrong generation ...).
    pub violations: Vec<String>,
    pub client_connected: bool,
    pub client_disconnected: bool,
    pub bound_globals: Vec<String>,
    /// `wl_pointer.set_cursor` requests: (has surface, hotspot x, hotspot y).
    pub cursor_sets: Vec<(bool, i32, i32)>,
    /// Newest buffer committed to a non-session surface: (width, height, ARGB8888 bytes).
    pub cursor_image: Option<(u32, u32, Vec<u8>)>,
}

pub enum Control {
    AddOutput(String, OutputSpec),
    ChangeOutput(String, OutputSpec),
    RemoveOutput(String),
    Reconfigure(String, OutputSpec),
    InputState(bool),
    PointerEnter(String, f64, f64),
    PointerMotion(f64, f64),
    PointerButton(u32, bool),
    PointerAxisDiscrete(i32, i32),
    PointerLeave,
    KeyboardEnter(String),
    Key(u32, bool),
    KeyboardLeave,
    Close,
    Shutdown,
}

struct OutputRec {
    spec: OutputSpec,
    surface: Option<wl_surface::WlSurface>,
    role: Option<BsmSessionSurfaceV1>,
    serial: u32,
    acked: u32,
    seq: u64,
}

struct SurfaceData {
    pending: Option<(wl_buffer::WlBuffer, Vec<(i32, i32, i32, i32)>)>,
    callbacks: Vec<wl_callback::WlCallback>,
    role_output: Option<String>,
}

pub struct BufferData {
    map: Arc<Mmap>,
    offset: usize,
    width: i32,
    height: i32,
    stride: i32,
}

struct State {
    shared: Arc<Mutex<Shared>>,
    generation: String,
    host: Option<BsmSessionHostV1>,
    outputs: HashMap<String, OutputRec>,
    surfaces: HashMap<wayland_server::backend::ObjectId, Arc<Mutex<SurfaceData>>>,
    pointers: Vec<wl_pointer::WlPointer>,
    keyboards: Vec<wl_keyboard::WlKeyboard>,
    touches: Vec<wl_touch::WlTouch>,
    serial: u32,
    input_active: bool,
}

impl State {
    fn next_serial(&mut self) -> u32 {
        self.serial += 1;
        self.serial
    }
    fn violation(&self, s: String) {
        self.shared.lock().unwrap().violations.push(s);
    }
    fn send_output_added(&self, name: &str) {
        if let (Some(host), Some(o)) = (&self.host, self.outputs.get(name)) {
            let s = o.spec;
            host.output_added(name.to_owned(), s.x, s.y, s.width, s.height, s.scale_120, s.refresh_mhz, s.transform);
        }
    }
    fn send_configure(&mut self, name: &str) {
        let serial = self.next_serial();
        let Some(o) = self.outputs.get_mut(name) else { return };
        o.serial = serial;
        if let Some(role) = &o.role {
            let (w, h) = o.spec.pixels();
            role.configure(serial, w, h, o.spec.scale_120, o.spec.transform);
            self.shared.lock().unwrap().configures.push((name.to_owned(), serial, w, h, o.spec.scale_120));
        }
    }
}

struct ClientState;
impl ClientData for ClientState {
    fn initialized(&self, _: wayland_server::backend::ClientId) {}
    fn disconnected(&self, _: wayland_server::backend::ClientId, _: wayland_server::backend::DisconnectReason) {}
}

// ----- globals -----

macro_rules! global {
    ($iface:ty, $name:expr) => {
        impl GlobalDispatch<$iface, ()> for State {
            fn bind(state: &mut Self, _: &DisplayHandle, _: &Client, r: New<$iface>, _: &(), init: &mut DataInit<'_, Self>) {
                state.shared.lock().unwrap().bound_globals.push($name.to_owned());
                init.init(r, ());
            }
        }
    };
}
global!(wl_compositor::WlCompositor, "wl_compositor");
global!(wl_shm::WlShm, "wl_shm");
impl GlobalDispatch<wl_seat::WlSeat, ()> for State {
    fn bind(state: &mut Self, _: &DisplayHandle, _: &Client, r: New<wl_seat::WlSeat>, _: &(), init: &mut DataInit<'_, Self>) {
        state.shared.lock().unwrap().bound_globals.push("wl_seat".to_owned());
        let seat = init.init(r, ());
        seat.capabilities(wl_seat::Capability::Pointer | wl_seat::Capability::Keyboard | wl_seat::Capability::Touch);
    }
}

impl GlobalDispatch<BsmSessionHostV1, ()> for State {
    fn bind(state: &mut Self, _: &DisplayHandle, _: &Client, r: New<BsmSessionHostV1>, _: &(), init: &mut DataInit<'_, Self>) {
        let host = init.init(r, ());
        {
            let mut sh = state.shared.lock().unwrap();
            sh.client_connected = true;
            sh.bound_globals.push("bsm_session_host_v1".into());
        }
        // SAFETY: getuid never fails.
        host.identity("standin-session".into(), state.generation.clone(), unsafe { libc::getuid() });
        state.host = Some(host);
        let names: Vec<String> = state.outputs.keys().cloned().collect();
        for n in names {
            state.send_output_added(&n);
        }
        let active = state.input_active as u32;
        state.host.as_ref().unwrap().input_state(active);
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for State {
    fn request(state: &mut Self, _: &Client, _: &wl_compositor::WlCompositor, req: wl_compositor::Request, _: &(), _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        match req {
            wl_compositor::Request::CreateSurface { id } => {
                let data = Arc::new(Mutex::new(SurfaceData { pending: None, callbacks: vec![], role_output: None }));
                let s = init.init(id, data.clone());
                state.surfaces.insert(s.id(), data);
            }
            wl_compositor::Request::CreateRegion { id } => {
                init.init(id, ());
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_region::WlRegion, ()> for State {
    fn request(_: &mut Self, _: &Client, _: &wl_region::WlRegion, _: wl_region::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn request(_: &mut Self, _: &Client, _: &wl_callback::WlCallback, _: wl_callback::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

impl Dispatch<wl_surface::WlSurface, Arc<Mutex<SurfaceData>>> for State {
    fn request(state: &mut Self, _: &Client, surface: &wl_surface::WlSurface, req: wl_surface::Request, data: &Arc<Mutex<SurfaceData>>, _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        use wl_surface::Request as R;
        match req {
            R::Attach { buffer, .. } => {
                let mut d = data.lock().unwrap();
                d.pending = buffer.map(|b| (b, vec![]));
            }
            R::DamageBuffer { x, y, width, height } => {
                if let Some((_, dmg)) = data.lock().unwrap().pending.as_mut() {
                    dmg.push((x, y, width, height));
                }
            }
            R::Frame { callback } => {
                let cb = init.init(callback, ());
                data.lock().unwrap().callbacks.push(cb);
            }
            R::Commit => {
                let (role_output, pending, cbs) = {
                    let mut d = data.lock().unwrap();
                    (d.role_output.clone(), d.pending.take(), std::mem::take(&mut d.callbacks))
                };
                for cb in cbs {
                    cb.done(0);
                }
                let Some((buffer, damage)) = pending else { return };
                let Some(bd) = buffer.data::<BufferData>() else { return };
                let mut pixels = Vec::with_capacity((bd.width * bd.height * 4) as usize);
                for y in 0..bd.height as usize {
                    let s = bd.offset + y * bd.stride as usize;
                    pixels.extend_from_slice(&bd.map[s..s + bd.width as usize * 4]);
                }
                let (w, h) = (bd.width as u32, bd.height as u32);
                buffer.release();
                let Some(name) = role_output else {
                    // A buffer on a non-session surface: the cursor image.
                    state.shared.lock().unwrap().cursor_image = Some((w, h, pixels));
                    return;
                };
                let Some(o) = state.outputs.get_mut(&name) else { return };
                let after_ack = o.acked != 0 && o.acked == o.serial;
                let (ew, eh) = o.spec.pixels();
                o.seq += 1;
                let seq = o.seq;
                if !after_ack {
                    let msg = format!("{name}: buffer committed before the ack of configure {} (acked {})", o.serial, o.acked);
                    state.violation(msg);
                }
                if (w, h) != (ew, eh) && after_ack {
                    let msg = format!("{name}: buffer {w}x{h} but configure says {ew}x{eh}");
                    state.violation(msg);
                }
                let o = state.outputs.get(&name).unwrap();
                if let Some(role) = &o.role {
                    if after_ack {
                        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
                        role.presented(seq as u32, (t.as_secs() >> 32) as u32, t.as_secs() as u32, t.subsec_nanos());
                    }
                }
                state.shared.lock().unwrap().frames.push(Frame {
                    output: name,
                    seq,
                    width: w,
                    height: h,
                    pixels,
                    after_ack,
                    damage,
                    at: Instant::now(),
                });
            }
            R::Destroy => {
                state.surfaces.remove(&surface.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_shm::WlShm, ()> for State {
    fn request(_: &mut Self, _: &Client, _: &wl_shm::WlShm, req: wl_shm::Request, _: &(), _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        if let wl_shm::Request::CreatePool { id, fd, size } = req {
            // SAFETY: the client shares a memfd; reads may race its writes, which is what a compositor does.
            let map = unsafe { Mmap::map(&std::fs::File::from(fd)) }.expect("map shm pool");
            assert!(map.len() >= size as usize);
            init.init(id, Arc::new(map));
        }
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, Arc<Mmap>> for State {
    fn request(_: &mut Self, _: &Client, _: &wl_shm_pool::WlShmPool, req: wl_shm_pool::Request, map: &Arc<Mmap>, _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        if let wl_shm_pool::Request::CreateBuffer { id, offset, width, height, stride, .. } = req {
            init.init(id, BufferData { map: map.clone(), offset: offset as usize, width, height, stride });
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, BufferData> for State {
    fn request(_: &mut Self, _: &Client, _: &wl_buffer::WlBuffer, _: wl_buffer::Request, _: &BufferData, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn request(state: &mut Self, _: &Client, seat: &wl_seat::WlSeat, req: wl_seat::Request, _: &(), _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        match req {
            wl_seat::Request::GetPointer { id } => state.pointers.push(init.init(id, ())),
            wl_seat::Request::GetKeyboard { id } => {
                let k = init.init(id, ());
                // A tiny but valid keymap file.
                let km = b"xkb_keymap { xkb_keycodes { <a> = 38; }; xkb_types {}; xkb_compat {}; xkb_symbols {}; };\0";
                let fd = rustix::fs::memfd_create("km", rustix::fs::MemfdFlags::CLOEXEC).unwrap();
                let mut f = std::fs::File::from(fd);
                f.write_all(km).unwrap();
                k.keymap(wl_keyboard::KeymapFormat::XkbV1, f.as_fd(), km.len() as u32);
                state.keyboards.push(k);
            }
            wl_seat::Request::GetTouch { id } => state.touches.push(init.init(id, ())),
            _ => {}
        }
        let _ = seat;
    }
}
impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn request(state: &mut Self, _: &Client, _: &wl_pointer::WlPointer, req: wl_pointer::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        if let wl_pointer::Request::SetCursor { surface, hotspot_x, hotspot_y, .. } = req {
            state.shared.lock().unwrap().cursor_sets.push((surface.is_some(), hotspot_x, hotspot_y));
        }
    }
}
impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn request(_: &mut Self, _: &Client, _: &wl_keyboard::WlKeyboard, _: wl_keyboard::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}
impl Dispatch<wl_touch::WlTouch, ()> for State {
    fn request(_: &mut Self, _: &Client, _: &wl_touch::WlTouch, _: wl_touch::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}
impl Dispatch<wl_output::WlOutput, ()> for State {
    fn request(_: &mut Self, _: &Client, _: &wl_output::WlOutput, _: wl_output::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

impl Dispatch<BsmSessionHostV1, ()> for State {
    fn request(state: &mut Self, _: &Client, _: &BsmSessionHostV1, req: bsm_session_host_v1::Request, _: &(), _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        match req {
            bsm_session_host_v1::Request::GetSessionSurface { id, surface, output_name } => {
                let role = init.init(id, output_name.clone());
                if let Some(d) = state.surfaces.get(&surface.id()) {
                    d.lock().unwrap().role_output = Some(output_name.clone());
                }
                match state.outputs.get_mut(&output_name) {
                    Some(o) if o.surface.is_none() => {
                        o.surface = Some(surface);
                        o.role = Some(role);
                    }
                    _ => {
                        state.violation(format!("get_session_surface for unknown or constructed output {output_name}"));
                        return;
                    }
                }
                state.send_configure(&output_name);
            }
            bsm_session_host_v1::Request::Report { kind, output_name, a, b, generation } => {
                let rec = ReportRec {
                    kind: match kind {
                        WEnum::Value(k) => k as u32,
                        WEnum::Unknown(u) => u,
                    },
                    output: output_name,
                    a,
                    b,
                    generation: generation.clone(),
                    at: Instant::now(),
                };
                if generation != state.generation {
                    state.violation(format!("stale generation {generation} in report {rec:?}"));
                }
                state.shared.lock().unwrap().reports.push(rec);
            }
            bsm_session_host_v1::Request::Destroy => {}
        }
    }
}

impl Dispatch<BsmSessionSurfaceV1, String> for State {
    fn request(state: &mut Self, _: &Client, _: &BsmSessionSurfaceV1, req: bsm_session_surface_v1::Request, name: &String, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        if let bsm_session_surface_v1::Request::AckConfigure { serial } = req {
            if let Some(o) = state.outputs.get_mut(name) {
                if serial <= o.serial && serial > o.acked {
                    o.acked = serial;
                    state.shared.lock().unwrap().acks.push((name.clone(), serial));
                } else {
                    let msg = format!("{name}: bad ack {serial} (latest {}, acked {})", o.serial, o.acked);
                    state.violation(msg);
                }
            }
        }
    }
}

// ----- the driver -----

pub struct Standin {
    pub socket: PathBuf,
    pub shared: Arc<Mutex<Shared>>,
    tx: Sender<Control>,
    join: Option<JoinHandle<()>>,
}

impl Standin {
    pub fn start(dir: &Path, generation: &str) -> Standin {
        let socket = dir.join("bsm-session.sock");
        let _ = std::fs::remove_file(&socket);
        let listener = ListeningSocket::bind_absolute(socket.clone()).expect("bind stand-in socket");
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (tx, rx) = mpsc::channel::<Control>();
        let sh = shared.clone();
        let generation = generation.to_owned();
        let join = std::thread::spawn(move || serve(listener, rx, sh, generation));
        Standin { socket, shared, tx, join: Some(join) }
    }

    pub fn send(&self, c: Control) {
        let _ = self.tx.send(c);
    }

    pub fn add_output(&self, name: &str, spec: OutputSpec) {
        self.send(Control::AddOutput(name.into(), spec));
    }

    /// Poll `f` over the shared state until it returns true or the timeout expires.
    pub fn wait_for(&self, what: &str, timeout: Duration, f: impl Fn(&Shared) -> bool) {
        let start = Instant::now();
        loop {
            if f(&self.shared.lock().unwrap()) {
                return;
            }
            if start.elapsed() > timeout {
                panic!("timed out waiting for {what}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn try_wait_for(&self, timeout: Duration, f: impl Fn(&Shared) -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if f(&self.shared.lock().unwrap()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    pub fn stop(&mut self) {
        self.send(Control::Shutdown);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for Standin {
    fn drop(&mut self) {
        self.stop();
    }
}

fn serve(listener: ListeningSocket, rx: Receiver<Control>, shared: Arc<Mutex<Shared>>, generation: String) {
    let mut display: Display<State> = Display::new().unwrap();
    let dh = display.handle();
    dh.create_global::<State, wl_compositor::WlCompositor, _>(6, ());
    dh.create_global::<State, wl_shm::WlShm, _>(1, ());
    dh.create_global::<State, wl_seat::WlSeat, _>(8, ());
    dh.create_global::<State, BsmSessionHostV1, _>(1, ());
    let mut state = State {
        shared: shared.clone(),
        generation,
        host: None,
        outputs: HashMap::new(),
        surfaces: HashMap::new(),
        pointers: vec![],
        keyboards: vec![],
        touches: vec![],
        serial: 100,
        input_active: false,
    };
    let mut clients = Vec::new();
    loop {
        while let Ok(Some(stream)) = listener.accept() {
            clients.push(display.handle().insert_client(stream, Arc::new(ClientState)).unwrap());
        }
        while let Ok(c) = rx.try_recv() {
            match c {
                Control::Shutdown => return,
                other => handle_control(&mut state, other),
            }
        }
        if display.dispatch_clients(&mut state).is_err() {
            break;
        }
        // A client that went away is noticed when its resources die.
        if state.host.as_ref().is_some_and(|h| !h.is_alive()) {
            shared.lock().unwrap().client_disconnected = true;
        }
        let _ = display.flush_clients();
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn handle_control(state: &mut State, c: Control) {
    match c {
        Control::AddOutput(name, spec) => {
            state.outputs.insert(name.clone(), OutputRec { spec, surface: None, role: None, serial: 0, acked: 0, seq: 0 });
            state.send_output_added(&name);
        }
        Control::ChangeOutput(name, spec) => {
            if let Some(o) = state.outputs.get_mut(&name) {
                o.spec = spec;
            }
            if let (Some(host), Some(o)) = (&state.host, state.outputs.get(&name)) {
                let s = o.spec;
                host.output_changed(name.clone(), s.x, s.y, s.width, s.height, s.scale_120, s.refresh_mhz, s.transform);
            }
        }
        Control::Reconfigure(name, spec) => {
            if let Some(o) = state.outputs.get_mut(&name) {
                o.spec = spec;
            }
            state.send_configure(&name);
        }
        Control::RemoveOutput(name) => {
            if let Some(host) = &state.host {
                host.output_removed(name.clone());
            }
            state.outputs.remove(&name);
        }
        Control::InputState(a) => {
            state.input_active = a;
            if let Some(h) = &state.host {
                h.input_state(a as u32);
            }
        }
        Control::PointerEnter(out, x, y) => {
            let serial = state.next_serial();
            if let Some(s) = state.outputs.get(&out).and_then(|o| o.surface.clone()) {
                for p in &state.pointers {
                    p.enter(serial, &s, x, y);
                    p.frame();
                }
            }
        }
        Control::PointerMotion(x, y) => {
            for p in &state.pointers {
                p.motion(0, x, y);
                p.frame();
            }
        }
        Control::PointerButton(b, pressed) => {
            let serial = state.next_serial();
            for p in &state.pointers {
                p.button(serial, 0, b, if pressed { wl_pointer::ButtonState::Pressed } else { wl_pointer::ButtonState::Released });
                p.frame();
            }
        }
        Control::PointerAxisDiscrete(x, y) => {
            for p in &state.pointers {
                if p.version() >= 8 {
                    if y != 0 {
                        p.axis_value120(wl_pointer::Axis::VerticalScroll, y);
                    }
                    if x != 0 {
                        p.axis_value120(wl_pointer::Axis::HorizontalScroll, x);
                    }
                }
                p.frame();
            }
        }
        Control::PointerLeave => {
            let serial = state.next_serial();
            for p in &state.pointers {
                if let Some(s) = state.outputs.values().find_map(|o| o.surface.clone()) {
                    p.leave(serial, &s);
                }
            }
        }
        Control::KeyboardEnter(out) => {
            let serial = state.next_serial();
            if let Some(s) = state.outputs.get(&out).and_then(|o| o.surface.clone()) {
                for k in &state.keyboards {
                    k.enter(serial, &s, vec![]);
                }
            }
        }
        Control::Key(code, pressed) => {
            let serial = state.next_serial();
            for k in &state.keyboards {
                k.key(serial, 0, code, if pressed { wl_keyboard::KeyState::Pressed } else { wl_keyboard::KeyState::Released });
            }
        }
        Control::KeyboardLeave => {
            let serial = state.next_serial();
            for k in &state.keyboards {
                if let Some(s) = state.outputs.values().find_map(|o| o.surface.clone()) {
                    k.leave(serial, &s);
                }
            }
        }
        Control::Close => {
            if let Some(h) = &state.host {
                h.closed();
            }
        }
        Control::Shutdown => {}
    }
}

/// Write XRGB8888 pixels as a PNG.
pub fn save_png(path: &Path, width: u32, height: u32, bgrx: &[u8]) {
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for px in bgrx.chunks_exact(4) {
        rgb.extend_from_slice(&[px[2], px[1], px[0]]);
    }
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgb).unwrap();
}
