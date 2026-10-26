//! The installed-system backend: libseat for the seat and VT switching, udev for
//! hotplug, DRM/KMS with GBM buffers, GLES rendering and libinput.
//!
//! This module is written against the Smithay 0.7 API and is compiled in CI, but it
//! cannot be exercised on a development host that already owns the seat. It is meant
//! to be run in a QEMU guest (see docs/bsm/compositor.md for what was and was not
//! verified). What it guarantees structurally, independent of the hardware:
//!
//! * The scene engine is told a frame is done only when its page flips completed
//!   (`Protected` therefore means "no longer on a scanout buffer").
//! * A session pause (VT switch away) stops rendering and input at once, and resume
//!   re-scans connectors and re-renders from scratch, never from a stale buffer.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use bsm_proto::scene::OutputInfo;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::format::FormatSet;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::Fourcc;
use smithay::backend::drm::compositor::FrameFlags;
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent, DrmNode, NodeType};
use smithay::backend::drm::compositor::DrmCompositor;
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::output::OutputModeSource;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{Color32F, ImportDma};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::{UdevBackend, UdevEvent, primary_gpu};
use smithay::reexports::calloop::RegistrationToken;
use smithay::reexports::drm::control::{self, Device as ControlDevice, ModeTypeFlags, connector, crtc};
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix::fs::OFlags;
use smithay::utils::{DeviceFd, Scale, Transform};

use crate::engine::Plan;
use crate::gfx::Gfx;
use crate::harness::CapturedFrame;
use crate::render::{build_elements, default_cursor_buffer};
use crate::state::State;

type GbmFb = GbmFramebufferExporter<DrmDeviceFd>;
type Alloc = GbmAllocator<DrmDeviceFd>;
type Compositor = DrmCompositor<Alloc, GbmFb, (), DrmDeviceFd>;

const COLOR_FORMATS: [Fourcc; 2] = [Fourcc::Argb8888, Fourcc::Xrgb8888];

struct DrmOutput {
    info: OutputInfo,
    connector: connector::Handle,
    compositor: Compositor,
    pending_flip: bool,
}

pub(crate) struct DrmGfx {
    /// Holds the seat for the life of the backend.
    #[allow(dead_code)]
    session: LibSeatSession,
    libinput: Libinput,
    device: DrmDevice,
    gbm: GbmDevice<DrmDeviceFd>,
    render_node: DrmNode,
    renderer: GlesRenderer,
    outputs: HashMap<crtc::Handle, DrmOutput>,
    cursor: MemoryRenderBuffer,
    active: bool,
    #[allow(dead_code)]
    device_id: libc::dev_t,
    #[allow(dead_code)]
    tokens: Vec<RegistrationToken>,
}

fn connector_name(info: &connector::Info) -> String {
    format!("{}-{}", info.interface().as_str(), info.interface_id())
}

impl DrmGfx {
    pub fn import_dmabuf(&mut self, d: &Dmabuf) -> bool {
        self.renderer.import_dmabuf(d, None).is_ok()
    }

    pub fn dmabuf_formats(&mut self) -> FormatSet {
        self.renderer.dmabuf_formats()
    }

    /// Turn the displays off or on without touching any lock state.
    pub fn set_dpms(&mut self, on: bool) {
        for out in self.outputs.values_mut() {
            let value = if on { 0 } else { 3 };
            let prop = self
                .device
                .get_properties(out.connector)
                .ok()
                .and_then(|props| {
                    let (ids, _) = props.as_props_and_values();
                    ids.iter()
                        .find(|id| {
                            self.device
                                .get_property(**id)
                                .map(|p| p.name().to_bytes() == b"DPMS")
                                .unwrap_or(false)
                        })
                        .copied()
                });
            match prop {
                Some(p) => {
                    if let Err(e) = self.device.set_property(out.connector, p, value) {
                        tracing::warn!("DPMS on {}: {e}", out.info.name);
                    }
                }
                None => tracing::warn!("{} has no DPMS property", out.info.name),
            }
            if on {
                let _ = out.compositor.reset_state();
            }
        }
    }

    pub fn render(&mut self, state: &State, plan: &Plan, _now_ms: u64) -> Result<(Vec<CapturedFrame>, bool)> {
        if !self.active {
            return Ok((Vec::new(), false));
        }
        let mut queued = false;
        let c = state.cover;
        for rec in &state.outputs {
            let Some((_, out)) = self.outputs.iter_mut().find(|(_, o)| o.info.name == rec.info.name) else {
                continue;
            };
            if out.pending_flip {
                continue;
            }
            let elements = build_elements(state, &mut self.renderer, rec, plan, Some(&self.cursor), state.frame_seq);
            match out.compositor.render_frame(
                &mut self.renderer,
                &elements,
                Color32F::new(c.r, c.g, c.b, 1.0),
                FrameFlags::DEFAULT,
            ) {
                Ok(res) => {
                    if !res.is_empty {
                        match out.compositor.queue_frame(()) {
                            Ok(()) => {
                                out.pending_flip = true;
                                queued = true;
                            }
                            Err(e) => tracing::warn!("queue_frame on {}: {e:?}", out.info.name),
                        }
                    }
                }
                Err(e) => tracing::warn!("render_frame on {}: {e:?}", out.info.name),
            }
        }
        Ok((Vec::new(), queued))
    }

    fn scan(&self) -> Result<Vec<(OutputInfo, connector::Handle, crtc::Handle, control::Mode)>> {
        let res = self.device.resource_handles().context("resource handles")?;
        let mut used: Vec<crtc::Handle> = Vec::new();
        let mut found = Vec::new();
        let mut x = 0i32;
        for &conn in res.connectors() {
            let info = self.device.get_connector(conn, true).context("connector")?;
            if info.state() != connector::State::Connected || info.modes().is_empty() {
                continue;
            }
            let mode = info
                .modes()
                .iter()
                .find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
                .copied()
                .unwrap_or(info.modes()[0]);
            // Keep the crtc an output already uses; otherwise take the first free one the
            // connector's encoders can drive.
            let existing = self
                .outputs
                .iter()
                .find(|(_, o)| o.connector == conn)
                .map(|(c, _)| *c);
            let mut crtc_choice = existing.filter(|c| !used.contains(c));
            if crtc_choice.is_none() {
                if let Some(enc) = info.current_encoder().and_then(|e| self.device.get_encoder(e).ok()) {
                    crtc_choice = enc.crtc().filter(|c| !used.contains(c));
                }
            }
            if crtc_choice.is_none() {
                'outer: for enc in info.encoders() {
                    let Ok(enc) = self.device.get_encoder(*enc) else { continue };
                    for c in res.filter_crtcs(enc.possible_crtcs()) {
                        if !used.contains(&c) {
                            crtc_choice = Some(c);
                            break 'outer;
                        }
                    }
                }
            }
            let Some(crtc) = crtc_choice else { continue };
            used.push(crtc);
            let (w, h) = mode.size();
            let oi = OutputInfo {
                name: connector_name(&info),
                x,
                y: 0,
                width: w as u32,
                height: h as u32,
                scale_120: 120,
                refresh_mhz: mode.vrefresh() * 1000,
                transform: 0,
            };
            x += w as i32;
            found.push((oi, conn, crtc, mode));
        }
        Ok(found)
    }

    fn make_compositor(
        &mut self,
        crtc: crtc::Handle,
        conn: connector::Handle,
        mode: control::Mode,
        info: &OutputInfo,
    ) -> Result<Compositor> {
        let surface = self.device.create_surface(crtc, mode, &[conn]).context("create_surface")?;
        let planes = self.device.planes(&crtc).ok();
        let allocator = GbmAllocator::new(self.gbm.clone(), GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
        let exporter = GbmFramebufferExporter::new(self.gbm.clone(), self.render_node.into());
        let formats = self.renderer.egl_context().dmabuf_render_formats().iter().copied().collect::<Vec<_>>();
        DrmCompositor::new(
            OutputModeSource::Static {
                size: (info.width as i32, info.height as i32).into(),
                scale: Scale::from(1.0),
                transform: Transform::Normal,
            },
            surface,
            planes,
            allocator,
            exporter,
            COLOR_FORMATS,
            formats,
            self.device.cursor_size(),
            Some(self.gbm.clone()),
        )
        .map_err(|e| anyhow::anyhow!("drm compositor: {e:?}"))
    }

    /// Reconcile `self.outputs` with the connected connectors and return the output set.
    fn sync_outputs(&mut self) -> Result<Vec<OutputInfo>> {
        let found = self.scan()?;
        // Drop outputs that disappeared.
        self.outputs
            .retain(|crtc, o| found.iter().any(|(i, c, cr, _)| i.name == o.info.name && *c == o.connector && cr == crtc));
        for (info, conn, crtc, mode) in &found {
            if let Some(o) = self.outputs.get_mut(crtc) {
                o.info = info.clone();
                continue;
            }
            match self.make_compositor(*crtc, *conn, *mode, info) {
                Ok(compositor) => {
                    self.outputs.insert(
                        *crtc,
                        DrmOutput { info: info.clone(), connector: *conn, compositor, pending_flip: false },
                    );
                }
                Err(e) => tracing::warn!("cannot drive {}: {e:#}", info.name),
            }
        }
        let mut infos: Vec<OutputInfo> = self.outputs.values().map(|o| o.info.clone()).collect();
        infos.sort_by_key(|i| i.x);
        Ok(infos)
    }
}

impl State {
    fn with_drm<R>(&mut self, f: impl FnOnce(&mut DrmGfx) -> R) -> Option<R> {
        match self.gfx.as_mut() {
            Some(Gfx::Drm(g)) => Some(f(g)),
            _ => None,
        }
    }

    /// Hotplug or resume: re-read connectors and publish the new layout.
    pub(crate) fn drm_rescan(&mut self) {
        match self.with_drm(|g| g.sync_outputs()) {
            Some(Ok(infos)) => self.set_outputs(infos),
            Some(Err(e)) => tracing::warn!("connector scan failed: {e:#}"),
            None => {}
        }
    }

    fn drm_vblank(&mut self, crtc: crtc::Handle) {
        let idle = self
            .with_drm(|g| {
                if let Some(o) = g.outputs.get_mut(&crtc) {
                    if let Err(e) = o.compositor.frame_submitted() {
                        tracing::warn!("frame_submitted on {}: {e:?}", o.info.name);
                    }
                    o.pending_flip = false;
                }
                g.outputs.values().all(|o| !o.pending_flip)
            })
            .unwrap_or(true);
        if idle {
            self.complete_awaiting();
        }
    }

    fn drm_session_event(&mut self, ev: SessionEvent) {
        match ev {
            SessionEvent::PauseSession => {
                tracing::info!("session paused (VT switched away)");
                self.with_drm(|g| {
                    g.active = false;
                    g.libinput.suspend();
                    g.device.pause();
                    for o in g.outputs.values_mut() {
                        o.pending_flip = false;
                    }
                });
                // Nothing will flip while paused: do not hold the engine hostage.
                self.complete_awaiting();
            }
            SessionEvent::ActivateSession => {
                tracing::info!("session activated");
                let res = self
                    .with_drm(|g| {
                        let r = g.libinput.resume().map_err(|_| anyhow::anyhow!("libinput resume"));
                        let a = g.device.activate(false).map_err(|e| anyhow::anyhow!("drm activate: {e:?}"));
                        g.active = true;
                        for o in g.outputs.values_mut() {
                            o.pending_flip = false;
                            let _ = o.compositor.reset_state();
                        }
                        r.and(a)
                    })
                    .unwrap_or(Ok(()));
                if let Err(e) = res {
                    tracing::error!("resume failed: {e:#}");
                }
                self.drm_rescan();
                self.needs_redraw = true;
            }
        }
    }
}

pub(crate) fn init_drm(state: &mut State, device: Option<PathBuf>) -> Result<()> {
    let (mut session, notifier) = LibSeatSession::new().map_err(|e| anyhow::anyhow!("libseat: {e:?}"))?;
    let seat = session.seat();
    let path = match device {
        Some(p) => p,
        None => primary_gpu(&seat)
            .context("udev")?
            .context("no GPU on this seat")?,
    };
    let fd = session
        .open(&path, OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK)
        .map_err(|e| anyhow::anyhow!("open {}: {e:?}", path.display()))?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (device, notifier_drm) = DrmDevice::new(fd.clone(), true).map_err(|e| anyhow::anyhow!("drm device: {e:?}"))?;
    let gbm = GbmDevice::new(fd.clone()).context("gbm device")?;
    // SAFETY: the display wraps a live GBM device that outlives it (both are owned by DrmGfx).
    let egl = unsafe { EGLDisplay::new(gbm.clone()) }.map_err(|e| anyhow::anyhow!("egl display: {e:?}"))?;
    let ctx = EGLContext::new(&egl).map_err(|e| anyhow::anyhow!("egl context: {e:?}"))?;
    // SAFETY: the context belongs to the display above and is used on this thread only.
    let renderer = unsafe { GlesRenderer::new(ctx) }.map_err(|e| anyhow::anyhow!("gles: {e:?}"))?;
    let node = DrmNode::from_file(&fd).context("drm node")?;
    let render_node = node.node_with_type(NodeType::Render).and_then(Result::ok).unwrap_or(node);
    let device_id = node.dev_id();

    let libinput_iface = LibinputSessionInterface::from(session.clone());
    let mut libinput = Libinput::new_with_udev(libinput_iface);
    libinput
        .udev_assign_seat(&seat)
        .map_err(|_| anyhow::anyhow!("libinput cannot use seat {seat}"))?;
    let input_backend = LibinputInputBackend::new(libinput.clone());

    let mut tokens = Vec::new();
    let session_for_vt = session.clone();

    tokens.push(
        state
            .handle
            .insert_source(notifier, |ev, _, state| state.drm_session_event(ev))
            .map_err(|e| anyhow::anyhow!("session source: {e}"))?,
    );
    tokens.push(
        state
            .handle
            .insert_source(notifier_drm, |ev, _, state| match ev {
                DrmEvent::VBlank(crtc) => state.drm_vblank(crtc),
                DrmEvent::Error(e) => tracing::error!("drm error: {e:?}"),
            })
            .map_err(|e| anyhow::anyhow!("drm source: {e}"))?,
    );
    let mut vt_session = session_for_vt;
    tokens.push(
        state
            .handle
            .insert_source(input_backend, move |event, _, state| {
                if let smithay::backend::input::InputEvent::DeviceAdded { device } = &event {
                    use smithay::reexports::input::DeviceConfigError;
                    let mut device = device.clone();
                    tracing::info!(
                        "input device added: {} (keyboard: {}, pointer: {})",
                        device.name(),
                        device.has_capability(smithay::reexports::input::DeviceCapability::Keyboard),
                        device.has_capability(smithay::reexports::input::DeviceCapability::Pointer),
                    );
                    if device.config_tap_finger_count() > 0 {
                        let _: Result<_, DeviceConfigError> = device.config_tap_set_enabled(true);
                    }
                }
                if let Some(vt) = state.process_input_event(event, true) {
                    if let Err(e) = vt_session.change_vt(vt) {
                        tracing::warn!("VT switch to {vt} failed: {e:?}");
                    }
                }
            })
            .map_err(|e| anyhow::anyhow!("libinput source: {e}"))?,
    );
    let udev = UdevBackend::new(&seat).context("udev backend")?;
    tokens.push(
        state
            .handle
            .insert_source(udev, move |ev, _, state| match ev {
                UdevEvent::Changed { device_id: d } if d == device_id => state.drm_rescan(),
                UdevEvent::Removed { device_id: d } if d == device_id => {
                    tracing::error!("the display GPU was removed");
                    state.stopped = true;
                }
                _ => {}
            })
            .map_err(|e| anyhow::anyhow!("udev source: {e}"))?,
    );

    let mut gfx = DrmGfx {
        session,
        libinput,
        device,
        gbm,
        render_node,
        renderer,
        outputs: HashMap::new(),
        cursor: default_cursor_buffer(),
        active: true,
        device_id,
        tokens,
    };
    let infos = gfx.sync_outputs()?;
    anyhow::ensure!(!infos.is_empty(), "no connected display on {}", path.display());
    state.gfx = Some(Gfx::Drm(gfx));
    state.set_outputs(infos);
    Ok(())
}
