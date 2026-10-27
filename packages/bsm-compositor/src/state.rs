//! The compositor state: Wayland globals with role-gated visibility, outputs, the
//! scene engine, the input owner, and the event loop entry point.

use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::Sender;
use std::time::Instant;

use anyhow::{Context, Result};
use bsm_proto::scene::{CoverColor, OutputInfo, SceneCommand, SceneEvent};
use bsm_proto::state::{Generation, InputOwner, SessionId};
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::delegate_dmabuf;
use smithay::delegate_layer_shell;
use smithay::desktop::{WindowSurfaceType, layer_map_for_output};
use smithay::input::pointer::{CursorImageStatus, PointerHandle};
use smithay::input::keyboard::KeyboardHandle;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{
    EventLoop, Interest, LoopHandle, Mode as PollMode, PostAction, RegistrationToken,
    channel::Event as ChannelEvent,
};
use smithay::reexports::wayland_protocols::wp::fractional_scale::v1::server::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation::WpPresentation;
use smithay::reexports::wayland_protocols::wp::viewporter::server::wp_viewporter::WpViewporter;
use smithay::reexports::wayland_protocols::xdg::xdg_output::zv1::server::zxdg_output_manager_v1::ZxdgOutputManagerV1;
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_compositor::WlCompositor;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_seat::WlSeat;
use smithay::reexports::wayland_server::protocol::wl_shm::WlShm;
use smithay::reexports::wayland_server::protocol::wl_subcompositor::WlSubcompositor;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Display, DisplayHandle, GlobalDispatch, New,
};
use smithay::utils::{Logical, Point, Transform};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    CompositorClientState, CompositorHandler, CompositorState, get_parent,
};
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use smithay::wayland::fractional_scale::{
    FractionalScaleHandler, FractionalScaleManagerState, with_fractional_scale,
};
use smithay::wayland::output::{OutputHandler, OutputManagerState, WlOutputData};
use smithay::wayland::presentation::PresentationState;
use smithay::wayland::seat::SeatGlobalData;
use smithay::wayland::shell::wlr_layer::{
    Layer, LayerSurface as WlrLayerSurface, WlrLayerShellHandler, WlrLayerShellState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::wayland::viewporter::ViewporterState;

use crate::engine::{Engine, Probe, SessionKey};
use crate::gfx::Gfx;
use crate::host::SessionRec;
use crate::peer::Peer;
use crate::roles::{ClientState, GlobalKind, Role, client_may_see};
use crate::{Backend, Config, HeadlessDrive};

/// Messages from client callbacks (which run inside Wayland dispatch and cannot touch
/// the state) to the loop.
pub(crate) enum Internal {
    ClientGone { id: ClientId, role: Role },
}

/// A connection held outside the Wayland display until its credentials can be checked.
pub(crate) struct PendingConn {
    pub stream: UnixStream,
    pub peer: Peer,
    pub since: Instant,
}

pub(crate) struct GreeterAuth {
    pub generation: Generation,
    pub uid: u32,
    pub pid: u32,
}

#[derive(Default)]
pub(crate) struct GreeterSlot {
    pub auth: Option<GreeterAuth>,
    pub client: Option<ClientId>,
    /// Generation and pid of the connected greeter.
    pub connected: Option<(Generation, u32)>,
    pub pending: Vec<PendingConn>,
}

pub(crate) struct OutputRec {
    pub info: OutputInfo,
    pub output: Output,
    pub global: GlobalId,
}

pub(crate) struct Awaiting {
    pub plan: crate::engine::Plan,
    pub now_ms: u64,
    pub since: Instant,
    /// Some output could not be composed or queued for this plan: when the rest completes,
    /// the frame is a failure, never an acknowledgement.
    pub failed: bool,
    /// The cover frame that precedes switching the displays off (not a scene frame).
    pub blank: bool,
    /// The frames that will be on screen once the flips complete (headless test hook).
    pub frames: Vec<crate::harness::CapturedFrame>,
}

/// After this many failed frames in a row the running transition is given up (`Failed`).
const MAX_FRAME_FAILURES: u32 = 5;
const FRAME_RETRY: std::time::Duration = std::time::Duration::from_millis(100);
pub(crate) const FLIP_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

pub(crate) enum Clock {
    Wall(Instant),
    Manual(u64),
}

impl Clock {
    pub fn now_ms(&self) -> u64 {
        match self {
            Clock::Wall(t) => t.elapsed().as_millis() as u64,
            Clock::Manual(ms) => *ms,
        }
    }
}

pub(crate) struct State {
    pub cfg: crate::broker::RuntimeCfg,
    pub dh: DisplayHandle,
    pub handle: LoopHandle<'static, State>,
    pub stopped: bool,
    pub evt_tx: Sender<SceneEvent>,
    pub internal_tx: smithay::reexports::calloop::channel::Sender<Internal>,
    pub clock: Clock,
    pub backend: Backend,

    pub compositor_state: CompositorState,
    pub shm_state: ShmState,
    // The states below own their globals; dropping them would remove the global.
    #[allow(dead_code)]
    pub output_manager_state: OutputManagerState,
    pub layer_shell_state: WlrLayerShellState,
    pub xdg_shell_state: XdgShellState,
    #[allow(dead_code)]
    pub viewporter_state: ViewporterState,
    #[allow(dead_code)]
    pub fractional_scale_state: FractionalScaleManagerState,
    #[allow(dead_code)]
    pub presentation_state: PresentationState,
    pub dmabuf_state: DmabufState,
    pub dmabuf_global: Option<DmabufGlobal>,
    pub seat_state: SeatState<State>,
    #[allow(dead_code)]
    pub seat: Seat<State>,
    pub keyboard: KeyboardHandle<State>,
    pub pointer: PointerHandle<State>,

    pub outputs: Vec<OutputRec>,
    pub greeter: GreeterSlot,
    pub greeter_listener: Option<RegistrationToken>,
    pub sessions: HashMap<SessionId, SessionRec>,
    pub surface_index: HashMap<WlSurface, (SessionId, String)>,

    pub engine: Engine,
    pub last_plan: crate::engine::Plan,
    pub cover: CoverColor,

    pub owner: InputOwner,
    pub pending_owner: Option<InputOwner>,
    pub pointer_pos: Point<f64, Logical>,
    pub cursor: CursorImageStatus,
    pub greeter_kbd_focus: Option<WlSurface>,

    pub gfx: Option<Gfx>,
    pub dpms_on: bool,
    pub frame_seq: u64,
    /// Frame-pacing log (`BSM_LOG=bsm_frames=debug`): when the frame in flight began rendering and when the last one completed.
    pub frame_started: Option<Instant>,
    pub last_frame_done: Option<Instant>,
    pub needs_redraw: bool,
    pub awaiting: Option<Awaiting>,
    /// Consecutive frames that did not (fully) reach the screen.
    pub frame_failures: u32,
    /// Do not try to compose again before this instant (failed frame back-off).
    pub retry_at: Option<Instant>,
    /// The displays are to be switched off once a cover frame is on the scanout buffers.
    pub blanking: bool,
    /// The first key since the last focus change has been logged.
    pub key_logged: bool,
    pub harness_frames: Vec<crate::harness::CapturedFrame>,
    pub dump_dir: Option<std::path::PathBuf>,
}

// ---------------------------------------------------------------------------------
// Role-gated globals. Smithay's delegate macros create ungated global dispatchers, so
// the global halves are written here with `can_view` and forward `bind` to Smithay.

macro_rules! gated_global {
    ($iface:ty, $udata:ty, $to:ty, $kind:expr) => {
        impl GlobalDispatch<$iface, $udata> for State {
            fn bind(
                state: &mut State,
                dh: &DisplayHandle,
                client: &Client,
                resource: New<$iface>,
                global_data: &$udata,
                data_init: &mut DataInit<'_, State>,
            ) {
                <$to as GlobalDispatch<$iface, $udata, State>>::bind(
                    state, dh, client, resource, global_data, data_init,
                )
            }
            fn can_view(client: Client, _global_data: &$udata) -> bool {
                client_may_see(&client, $kind)
            }
        }
    };
}

gated_global!(WlCompositor, (), CompositorState, GlobalKind::WlCompositor);
gated_global!(WlSubcompositor, (), CompositorState, GlobalKind::WlSubcompositor);
gated_global!(WlShm, (), ShmState, GlobalKind::WlShm);
gated_global!(WlSeat, SeatGlobalData<State>, SeatState<State>, GlobalKind::WlSeat);
gated_global!(WlOutput, WlOutputData, OutputManagerState, GlobalKind::WlOutput);
gated_global!(ZxdgOutputManagerV1, (), OutputManagerState, GlobalKind::XdgOutput);
gated_global!(XdgWmBase, (), XdgShellState, GlobalKind::XdgShell);
gated_global!(WpViewporter, (), ViewporterState, GlobalKind::Viewporter);
gated_global!(
    WpFractionalScaleManagerV1,
    (),
    FractionalScaleManagerState,
    GlobalKind::FractionalScale
);
gated_global!(WpPresentation, u32, PresentationState, GlobalKind::Presentation);

use smithay::reexports::wayland_server::delegate_dispatch;
use smithay::reexports::wayland_server::protocol::{wl_region, wl_shm_pool, wl_subsurface, wl_callback, wl_pointer, wl_keyboard, wl_touch};
use smithay::wayland::compositor::{RegionUserData, SubsurfaceUserData, SurfaceUserData};
use smithay::wayland::shm::{ShmBufferUserData, ShmPoolUserData};
use smithay::wayland::viewporter::ViewportState;
use smithay::reexports::wayland_protocols::xdg::shell::server::{
    xdg_popup::XdgPopup, xdg_positioner::XdgPositioner, xdg_surface::XdgSurface,
    xdg_toplevel::XdgToplevel, xdg_wm_base::XdgWmBase,
};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgPositionerUserData, XdgShellHandler, XdgWmBaseUserData,
    XdgShellState, XdgShellSurfaceUserData, XdgSurfaceUserData,
};
use smithay::wayland::output::{OutputUserData, XdgOutputUserData};
use smithay::reexports::wayland_protocols::xdg::xdg_output::zv1::server::zxdg_output_v1::ZxdgOutputV1;
use smithay::reexports::wayland_protocols::wp::viewporter::server::wp_viewport::WpViewport;
use smithay::reexports::wayland_protocols::wp::fractional_scale::v1::server::wp_fractional_scale_v1::WpFractionalScaleV1;
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback::WpPresentationFeedback;
use smithay::reexports::wayland_server::Weak;
use smithay::wayland::presentation::PresentationFeedbackState;

delegate_dispatch!(State: [WlCompositor: ()] => CompositorState);
delegate_dispatch!(State: [WlSurface: SurfaceUserData] => CompositorState);
delegate_dispatch!(State: [wl_region::WlRegion: RegionUserData] => CompositorState);
delegate_dispatch!(State: [wl_callback::WlCallback: ()] => CompositorState);
delegate_dispatch!(State: [WlSubcompositor: ()] => CompositorState);
delegate_dispatch!(State: [wl_subsurface::WlSubsurface: SubsurfaceUserData] => CompositorState);
delegate_dispatch!(State: [WlShm: ()] => ShmState);
delegate_dispatch!(State: [wl_shm_pool::WlShmPool: ShmPoolUserData] => ShmState);
delegate_dispatch!(State: [WlBuffer: ShmBufferUserData] => ShmState);
delegate_dispatch!(State: [WlSeat: smithay::wayland::seat::SeatUserData<State>] => SeatState<State>);
delegate_dispatch!(State: [wl_pointer::WlPointer: smithay::wayland::seat::PointerUserData<State>] => SeatState<State>);
delegate_dispatch!(State: [wl_keyboard::WlKeyboard: smithay::wayland::seat::KeyboardUserData<State>] => SeatState<State>);
delegate_dispatch!(State: [wl_touch::WlTouch: smithay::wayland::seat::TouchUserData<State>] => SeatState<State>);
delegate_dispatch!(State: [WlOutput: OutputUserData] => OutputManagerState);
delegate_dispatch!(State: [ZxdgOutputManagerV1: ()] => OutputManagerState);
delegate_dispatch!(State: [ZxdgOutputV1: XdgOutputUserData] => OutputManagerState);
delegate_dispatch!(State: [WpViewporter: ()] => ViewporterState);
delegate_dispatch!(State: [WpViewport: ViewportState] => ViewportState);
delegate_dispatch!(State: [WpFractionalScaleManagerV1: ()] => FractionalScaleManagerState);
delegate_dispatch!(State: [WpFractionalScaleV1: Weak<WlSurface>] => FractionalScaleManagerState);
delegate_dispatch!(State: [WpPresentation: u32] => PresentationState);
delegate_dispatch!(State: [WpPresentationFeedback: ()] => PresentationFeedbackState);
delegate_layer_shell!(State);
delegate_dispatch!(State: [XdgWmBase: XdgWmBaseUserData] => XdgShellState);
delegate_dispatch!(State: [XdgPositioner: XdgPositionerUserData] => XdgShellState);
delegate_dispatch!(State: [XdgPopup: XdgShellSurfaceUserData] => XdgShellState);
delegate_dispatch!(State: [XdgSurface: XdgSurfaceUserData] => XdgShellState);
delegate_dispatch!(State: [XdgToplevel: XdgShellSurfaceUserData] => XdgShellState);
delegate_dmabuf!(State);

// ---------------------------------------------------------------------------------
// Handlers

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client
            .get_data::<ClientState>()
            .expect("every client is inserted by bsm with a ClientState")
            .compositor
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<State>(surface);
        self.needs_redraw = true;
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        if self.surface_index.contains_key(&root) {
            self.host_surface_commit(&root);
            return;
        }
        self.greeter_surface_commit(&root);
    }
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl OutputHandler for State {}

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<State> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<State>, focused: Option<&WlSurface>) {
        let who = self.describe_surface(focused);
        tracing::info!("keyboard focus -> {who} (input owner {:?})", self.owner);
        self.key_logged = false;
    }

    fn cursor_image(&mut self, _seat: &Seat<State>, image: CursorImageStatus) {
        // Only the pointer-focused client can set the cursor (Smithay checks), which is
        // the input owner's client.
        self.cursor = image;
        self.needs_redraw = true;
    }
}

impl FractionalScaleHandler for State {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self
            .output_of_layer_surface(&surface)
            .or_else(|| self.outputs.first().map(|o| o.output.clone()))
            .map(|o| o.current_scale().fractional_scale())
            .unwrap_or(1.0);
        smithay::wayland::compositor::with_states(&surface, |states| {
            with_fractional_scale(states, |fs| fs.set_preferred_scale(scale));
        });
    }
}

impl WlrLayerShellHandler for State {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        wl_output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        // Only the greeter sees the layer-shell global, so this is the greeter's surface.
        let output = wl_output
            .as_ref()
            .and_then(Output::from_resource)
            .or_else(|| self.outputs.first().map(|o| o.output.clone()));
        let Some(output) = output else {
            surface.send_close();
            return;
        };
        let desktop = smithay::desktop::LayerSurface::new(surface, namespace);
        let mut map = layer_map_for_output(&output);
        if map.map_layer(&desktop).is_err() {
            desktop.layer_surface().send_close();
        }
        drop(map);
        self.needs_redraw = true;
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        for rec in &self.outputs {
            let mut map = layer_map_for_output(&rec.output);
            let found = map
                .layers()
                .find(|l| l.layer_surface() == &surface)
                .cloned();
            if let Some(l) = found {
                map.unmap_layer(&l);
            }
        }
        self.needs_redraw = true;
    }
}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    // Only the greeter sees `xdg_wm_base`, and only so that Qt can start. Toplevels and
    // popups are acknowledged and configured, never mapped into the scene.
    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        surface.send_configure();
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        let _ = surface.send_configure();
    }

    fn grab(&mut self, surface: PopupSurface, _seat: smithay::reexports::wayland_server::protocol::wl_seat::WlSeat, _serial: smithay::utils::Serial) {
        // No popup grabs: nothing is ever shown, so there is nothing to grab for.
        surface.send_popup_done();
    }

    fn reposition_request(&mut self, surface: PopupSurface, positioner: PositionerState, token: u32) {
        surface.with_pending_state(|s| {
            s.geometry = positioner.get_geometry();
            s.positioner = positioner;
        });
        surface.send_repositioned(token);
    }
}

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(&mut self, _global: &DmabufGlobal, dmabuf: Dmabuf, notifier: ImportNotifier) {
        let ok = match self.gfx.as_mut() {
            Some(g) => g.import_dmabuf(&dmabuf),
            None => false,
        };
        if ok {
            let _ = notifier.successful::<State>();
        } else {
            notifier.failed();
        }
    }
}

// ---------------------------------------------------------------------------------
// Probe for the engine

pub(crate) struct ProbeView<'a> {
    pub greeter_connected: bool,
    pub outputs: &'a [OutputRec],
    pub sessions: &'a HashMap<SessionId, SessionRec>,
}

fn surface_has_buffer(surface: &WlSurface) -> bool {
    smithay::backend::renderer::utils::with_renderer_surface_state(surface, |s| s.buffer().is_some())
        .unwrap_or(false)
}

impl Probe for ProbeView<'_> {
    fn greeter_presentable(&self) -> bool {
        if !self.greeter_connected || self.outputs.is_empty() {
            return false;
        }
        self.outputs.iter().all(|o| {
            layer_map_for_output(&o.output)
                .layers()
                .any(|l| surface_has_buffer(l.wl_surface()))
        })
    }

    fn session_exists(&self, key: &SessionKey) -> bool {
        self.sessions.get(&key.0).is_some_and(|s| s.generation == key.1)
    }

    fn session_presentable(&self, key: &SessionKey) -> bool {
        let Some(s) = self.sessions.get(&key.0).filter(|s| s.generation == key.1) else {
            return false;
        };
        s.connected
            && !self.outputs.is_empty()
            && self
                .outputs
                .iter()
                .all(|o| s.surfaces.get(&o.info.name).is_some_and(|hs| hs.ready(&o.info)))
    }
}

// ---------------------------------------------------------------------------------
// Construction and helpers

pub(crate) fn transform_from_u32(t: u32) -> Transform {
    match t {
        1 => Transform::_90,
        2 => Transform::_180,
        3 => Transform::_270,
        4 => Transform::Flipped,
        5 => Transform::Flipped90,
        6 => Transform::Flipped180,
        7 => Transform::Flipped270,
        _ => Transform::Normal,
    }
}

impl State {
    pub fn now_ms(&self) -> u64 {
        self.clock.now_ms()
    }

    pub fn emit(&self, e: SceneEvent) {
        // The manager hanging up is how it asks us to stop; the loop notices through the
        // command channel, so a failed send is not an error here.
        let _ = self.evt_tx.send(e);
    }

    pub fn output_of_layer_surface(&self, surface: &WlSurface) -> Option<Output> {
        self.outputs.iter().find_map(|o| {
            layer_map_for_output(&o.output)
                .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
                .map(|_| o.output.clone())
        })
    }

    /// Human readable name of the owner of a surface, for logs.
    pub fn describe_surface(&self, s: Option<&WlSurface>) -> String {
        let Some(s) = s else { return "nobody".into() };
        let mut root = s.clone();
        while let Some(p) = get_parent(&root) {
            root = p;
        }
        match self.surface_index.get(&root) {
            Some((sid, out)) => format!("session {sid} ({out})"),
            None => "greeter".into(),
        }
    }

    pub fn output_infos(&self) -> Vec<OutputInfo> {
        self.outputs.iter().map(|o| o.info.clone()).collect()
    }

    fn make_output(&self, info: &OutputInfo) -> OutputRec {
        let output = Output::new(
            info.name.clone(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "Bingux".into(),
                model: "bsm".into(),
            },
        );
        let mode = Mode { size: (info.width as i32, info.height as i32).into(), refresh: info.refresh_mhz as i32 };
        output.change_current_state(
            Some(mode),
            Some(transform_from_u32(info.transform)),
            Some(Scale::Fractional(info.scale_120.max(1) as f64 / 120.0)),
            Some((info.x, info.y).into()),
        );
        output.set_preferred(mode);
        let global = output.create_global::<State>(&self.dh);
        OutputRec { info: info.clone(), output, global }
    }

    /// Replace the output set: add, change and remove by name. Reports to the manager
    /// and to every hosted session (output events, new configure serials), and
    /// invalidates any evidence that no longer matches.
    pub fn set_outputs(&mut self, infos: Vec<OutputInfo>) {
        let before = self.output_infos();
        // Removed.
        let removed: Vec<String> = self
            .outputs
            .iter()
            .filter(|o| !infos.iter().any(|i| i.name == o.info.name))
            .map(|o| o.info.name.clone())
            .collect();
        for name in &removed {
            if let Some(pos) = self.outputs.iter().position(|o| &o.info.name == name) {
                let rec = self.outputs.remove(pos);
                // Layer surfaces on the vanished output are closed; the greeter re-creates
                // them on the screens that remain.
                {
                    let map = layer_map_for_output(&rec.output);
                    for l in map.layers() {
                        l.layer_surface().send_close();
                    }
                }
                self.dh.remove_global::<State>(rec.global);
            }
            self.host_output_removed(name);
        }
        for info in &infos {
            match self.outputs.iter().position(|o| o.info.name == info.name) {
                Some(pos) => {
                    if self.outputs[pos].info != *info {
                        let rec = &mut self.outputs[pos];
                        let mode = Mode {
                            size: (info.width as i32, info.height as i32).into(),
                            refresh: info.refresh_mhz as i32,
                        };
                        rec.output.change_current_state(
                            Some(mode),
                            Some(transform_from_u32(info.transform)),
                            Some(Scale::Fractional(info.scale_120.max(1) as f64 / 120.0)),
                            Some((info.x, info.y).into()),
                        );
                        rec.output.set_preferred(mode);
                        rec.info = info.clone();
                        layer_map_for_output(&rec.output).arrange();
                        if let Some(g) = self.gfx.as_mut() {
                            g.output_changed(info);
                        }
                        self.host_output_changed(info);
                    }
                }
                None => {
                    let rec = self.make_output(info);
                    self.outputs.push(rec);
                    if let Some(g) = self.gfx.as_mut() {
                        g.output_added(info);
                    }
                    self.host_output_added(info);
                }
            }
        }
        // Keep the order the caller gave.
        self.outputs
            .sort_by_key(|o| infos.iter().position(|i| i.name == o.info.name).unwrap_or(usize::MAX));
        if before != self.output_infos() {
            let outs = self.output_infos();
            self.emit(SceneEvent::OutputsChanged { outputs: outs });
        }
        self.needs_redraw = true;
    }

    /// Process loop messages from client callbacks.
    fn handle_internal(&mut self, msg: Internal) {
        match msg {
            Internal::ClientGone { id, role } => self.client_gone(id, role),
        }
    }

    fn run_gfx(&mut self, plan: &crate::engine::Plan, now: u64) -> (Vec<crate::harness::CapturedFrame>, crate::gfx::Outcome) {
        match self.gfx.take() {
            Some(mut g) => {
                let r = g.render(self, plan, now);
                self.gfx = Some(g);
                r
            }
            None => (Vec::new(), crate::gfx::Outcome::Failed("no renderer".into())),
        }
    }

    /// One composed frame: plan, render, answer, advance the engine. A frame is
    /// acknowledged (`Protected`, transition phases) only when it is known to be on the
    /// screen: immediately where there is nothing to wait for, otherwise when every queued
    /// page flip completed. A frame that did not reach every output acknowledges nothing and
    /// is retried.
    pub fn render_frame(&mut self) {
        // Input follows the scene even while a frame is in flight (review F5).
        self.reconcile_input();
        if self.awaiting.is_some() {
            // The previous frame has not reached the screen yet: one plan in flight at a time.
            return;
        }
        if !self.gfx.as_ref().is_none_or(|g| g.active()) {
            // The seat is paused (VT switched away): nothing can be shown, nothing is acked.
            return;
        }
        if let Some(t) = self.retry_at {
            if Instant::now() < t {
                return;
            }
            self.retry_at = None;
        }
        let now = self.now_ms();
        if self.blanking {
            // Switching the displays off: the scanout buffers get the cover first, so the
            // panel never holds a desktop or a lock screen while off (review F1e).
            let plan = crate::engine::Plan::empty();
            let (frames, outcome) = self.run_gfx(&plan, now);
            self.frame_seq += 1;
            self.harness_frames = frames.clone();
            self.needs_redraw = false;
            self.settle(plan, now, frames, outcome, true);
            return;
        }
        self.apply_pending_owner();
        let plan = {
            let probe = ProbeView {
                greeter_connected: self.greeter.connected.is_some(),
                outputs: &self.outputs,
                sessions: &self.sessions,
            };
            self.engine.plan(now, &probe)
        };
        // The plan may have changed what is foreground (failure, commit): fail closed on input.
        self.reconcile_input();
        self.last_plan = plan.clone();
        self.needs_redraw = false;
        self.frame_started = Some(Instant::now());
        let (frames, outcome) = if self.dpms_on {
            self.run_gfx(&plan, now)
        } else {
            // Displays are off over a cover (see above): nothing to show, nothing to wait for.
            let shown = self.gfx.as_ref().map(|g| g.scanout()).unwrap_or_default();
            (shown, crate::gfx::Outcome::Shown)
        };
        self.frame_seq += 1;
        self.harness_frames = frames.clone();
        self.settle(plan, now, frames, outcome, false);
    }

    fn settle(&mut self, plan: crate::engine::Plan, now: u64, frames: Vec<crate::harness::CapturedFrame>, outcome: crate::gfx::Outcome, blank: bool) {
        use crate::gfx::Outcome;
        match outcome {
            Outcome::Shown => self.frame_shown(plan, now, frames, blank),
            Outcome::Queued { failed } => {
                self.awaiting = Some(Awaiting { plan, now_ms: now, since: Instant::now(), failed, blank, frames });
            }
            Outcome::Failed(why) => self.frame_failed(&why, blank),
        }
    }

    fn frame_shown(&mut self, plan: crate::engine::Plan, now: u64, frames: Vec<crate::harness::CapturedFrame>, blank: bool) {
        self.frame_failures = 0;
        if let Some(crate::gfx::Gfx::Headless(g)) = self.gfx.as_mut() {
            if !frames.is_empty() {
                g.scanout = frames;
            }
        }
        if blank {
            if self.blanking {
                self.finish_blank();
            }
        } else {
            self.complete_frame(plan, now);
        }
    }

    fn finish_blank(&mut self) {
        self.blanking = false;
        self.dpms_on = false;
        if let Some(g) = self.gfx.as_mut() {
            g.set_dpms(false);
        }
    }

    /// The frame did not reach the screen (render error, an output skipped, a flip that
    /// never came). Nothing is acknowledged; the scene is composed and queued again after a
    /// short back-off, and a transition that cannot be shown is given up.
    pub fn frame_failed(&mut self, why: &str, blank: bool) {
        self.frame_failures += 1;
        tracing::warn!("frame {} did not reach the screen ({why}); failure {} in a row", self.frame_seq, self.frame_failures);
        if let Some(g) = self.gfx.as_mut() {
            g.frame_lost();
        }
        if !blank {
            self.engine.frame_failed();
            if self.frame_failures >= MAX_FRAME_FAILURES {
                self.engine.abort_transition();
                for e in self.engine.drain_events() {
                    self.emit(e);
                }
            }
        }
        self.needs_redraw = true;
        if !matches!(self.clock, Clock::Manual(_)) {
            self.retry_at = Some(Instant::now() + FRAME_RETRY);
        }
    }

    /// The frame reached the screen: send what clients are waiting for and let the engine
    /// advance, which is when `Protected` and the transition phases are reported.
    pub fn complete_frame(&mut self, plan: crate::engine::Plan, now: u64) {
        self.after_render(&plan, now);
        if tracing::enabled!(target: "bsm_frames", tracing::Level::DEBUG) {
            let done = Instant::now();
            let interval_us = self.last_frame_done.map(|t| done.duration_since(t).as_micros() as u64);
            let render_to_done_us = self.frame_started.map(|t| done.duration_since(t).as_micros() as u64);
            self.last_frame_done = Some(done);
            tracing::debug!(
                target: "bsm_frames",
                "frame seq={} interval_us={} render_to_done_us={} transition={}",
                self.frame_seq,
                interval_us.map_or(-1, |v| v as i64),
                render_to_done_us.map_or(-1, |v| v as i64),
                self.engine.transition_running(),
            );
        }
        self.engine.frame_done();
        for e in self.engine.drain_events() {
            self.emit(e);
        }
        // Commands and commits that arrived while the frame was in flight keep their request;
        // a protection that is still owed a frame, or a running transition, asks for more.
        self.needs_redraw |= self.engine.transition_running() || self.engine.has_queued_protect();
    }

    /// Page flips of the awaited frame completed.
    pub fn complete_awaiting(&mut self) {
        let Some(a) = self.awaiting.take() else { return };
        if a.failed {
            self.frame_failed("an output could not be composed or queued", a.blank);
            return;
        }
        self.frame_shown(a.plan, a.now_ms, a.frames, a.blank);
    }

    /// The awaited flip did not complete in time. It is a failed frame, never a shown one.
    pub fn flip_watchdog(&mut self, force: bool) {
        if let Some(a) = &self.awaiting {
            if force || a.since.elapsed() > FLIP_TIMEOUT {
                let blank = a.blank;
                self.awaiting = None;
                self.frame_failed("page flip did not complete in time", blank);
            }
        }
    }

    /// End of every loop iteration: flush clients.
    pub fn flush(&mut self) {
        let _ = self.dh.backend_handle().flush(None);
    }
}

// ---------------------------------------------------------------------------------
// Event loop

pub(crate) fn run(
    config: Config,
    cmd_rx: smithay::reexports::calloop::channel::Channel<SceneCommand>,
    evt_tx: Sender<SceneEvent>,
) -> Result<()> {
    let mut event_loop: EventLoop<'static, State> = EventLoop::try_new().context("event loop")?;
    let handle = event_loop.handle();
    let display: Display<State> = Display::new().context("wayland display")?;
    let dh = display.handle();

    let (internal_tx, internal_rx) = smithay::reexports::calloop::channel::channel::<Internal>();

    // The private session protocol. Only hosted-user connections can see it.
    dh.create_global::<State, crate::protocol::bsm_session_host_v1::BsmSessionHostV1, ()>(1, ());
    let compositor_state = CompositorState::new::<State>(&dh);
    let shm_state = ShmState::new::<State>(&dh, []);
    let output_manager_state = OutputManagerState::new_with_xdg_output::<State>(&dh);
    let layer_shell_state = WlrLayerShellState::new_with_filter::<State, _>(&dh, |c| {
        client_may_see(c, GlobalKind::LayerShell)
    });
    let xdg_shell_state = XdgShellState::new::<State>(&dh);
    let viewporter_state = ViewporterState::new::<State>(&dh);
    let fractional_scale_state = FractionalScaleManagerState::new::<State>(&dh);
    let presentation_state = PresentationState::new::<State>(&dh, libc::CLOCK_MONOTONIC as u32);
    let dmabuf_state = DmabufState::new();
    let mut seat_state = SeatState::new();
    let mut seat = seat_state.new_wl_seat(&dh, config.seat_name.clone());
    let keyboard = seat
        .add_keyboard(Default::default(), 200, 25)
        .context("keyboard")?;
    let pointer = seat.add_pointer();

    let clock = match (&config.backend, &config.headless.drive) {
        (Backend::Headless, HeadlessDrive::Stepped(_)) => Clock::Manual(0),
        _ => Clock::Wall(Instant::now()),
    };
    let rt = crate::broker::RuntimeCfg::from_config(&config);

    let mut state = State {
        cfg: rt,
        dh: dh.clone(),
        handle: handle.clone(),
        stopped: false,
        evt_tx,
        internal_tx,
        clock,
        backend: config.backend,
        compositor_state,
        shm_state,
        output_manager_state,
        layer_shell_state,
        xdg_shell_state,
        viewporter_state,
        fractional_scale_state,
        presentation_state,
        dmabuf_state,
        dmabuf_global: None,
        seat_state,
        seat,
        keyboard,
        pointer,
        outputs: Vec::new(),
        greeter: GreeterSlot::default(),
        greeter_listener: None,
        sessions: HashMap::new(),
        surface_index: HashMap::new(),
        engine: Engine::new(),
        last_plan: crate::engine::Plan::empty(),
        cover: config.cover,
        owner: InputOwner::None,
        pending_owner: None,
        pointer_pos: (0.0, 0.0).into(),
        cursor: CursorImageStatus::default_named(),
        greeter_kbd_focus: None,
        gfx: None,
        dpms_on: true,
        frame_seq: 0,
        frame_started: None,
        last_frame_done: None,
        needs_redraw: true,
        awaiting: None,
        frame_failures: 0,
        retry_at: None,
        blanking: false,
        key_logged: false,
        harness_frames: Vec::new(),
        dump_dir: config.frame_dump_dir.clone(),
    };

    // Wayland clients.
    handle
        .insert_source(Generic::new(display, Interest::READ, PollMode::Level), |_, display, state| {
            // SAFETY: the display is not dropped while the source lives.
            unsafe { display.get_mut().dispatch_clients(state).expect("dispatch clients") };
            Ok(PostAction::Continue)
        })
        .map_err(|e| anyhow::anyhow!("insert display source: {e}"))?;

    // Client callbacks to the loop.
    handle
        .insert_source(internal_rx, |ev, _, state| {
            if let ChannelEvent::Msg(m) = ev {
                state.handle_internal(m);
            }
        })
        .map_err(|e| anyhow::anyhow!("insert internal channel: {e}"))?;

    // Manager commands. A closed channel means the manager is gone: stop.
    handle
        .insert_source(cmd_rx, |ev, _, state| match ev {
            ChannelEvent::Msg(cmd) => state.handle_command(cmd),
            ChannelEvent::Closed => {
                tracing::info!("command channel closed, stopping");
                state.stopped = true;
            }
        })
        .map_err(|e| anyhow::anyhow!("insert command channel: {e}"))?;

    // Backend: outputs, renderer, input, and the frame driver.
    let Config { backend, headless, .. } = config;
    let HeadlessConfig { outputs, drive, draw_cursor } = headless;
    let mut ctl = None;
    match backend {
        Backend::Headless => {
            crate::gfx::init_headless(&mut state, outputs, drive, draw_cursor, &mut ctl)?;
        }
        Backend::Winit => {
            #[cfg(feature = "backend-winit")]
            crate::gfx::init_winit(&mut state)?;
            #[cfg(not(feature = "backend-winit"))]
            anyhow::bail!("built without the backend-winit feature");
        }
        Backend::Drm => {
            #[cfg(feature = "backend-drm")]
            crate::gfx::init_drm(&mut state, config.drm_device.clone())?;
            #[cfg(not(feature = "backend-drm"))]
            anyhow::bail!("built without the backend-drm feature");
        }
    }
    let _ = ctl;

    crate::broker::start_greeter_listener(&mut state)?;
    state.create_dmabuf_global();
    let ready = state.output_infos();
    state.emit(SceneEvent::Ready { outputs: ready });

    while !state_stopped(&state) {
        let timeout = state.loop_timeout();
        event_loop
            .dispatch(timeout, &mut state)
            .context("event loop dispatch")?;
        state.after_dispatch();
    }
    state.shutdown();
    Ok(())
}

use crate::HeadlessConfig;

fn state_stopped(state: &State) -> bool {
    state.stop_requested()
}

impl State {
    pub fn stop_requested(&self) -> bool {
        self.stopped
    }
}
