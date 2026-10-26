//! Rendering backends. `Headless` is complete and exercised by the test suite; `Winit`
//! and `Drm` live in their own modules and are compiled behind cargo features.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use bsm_proto::scene::OutputInfo;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::format::FormatSet;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::backend::renderer::pixman::PixmanRenderer;
use smithay::backend::renderer::{Bind, Color32F, ImportDma, Offscreen};
use smithay::reexports::calloop::channel::Event as ChannelEvent;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::utils::{Size, Transform};

use crate::engine::Plan;
use crate::harness::{CapturedFrame, FocusSnapshot, FocusTarget, HarnessCmd};
use crate::render::{build_elements, default_cursor_buffer};
use crate::state::{Clock, State};
use crate::{HeadlessDrive, roles::GlobalKind};

#[cfg(feature = "backend-drm")]
pub(crate) use crate::gfx_drm::{DrmGfx, init_drm};
#[cfg(feature = "backend-winit")]
pub(crate) use crate::gfx_winit::{WinitGfx, init_winit};

pub(crate) enum Gfx {
    Headless(HeadlessGfx),
    #[cfg(feature = "backend-winit")]
    Winit(WinitGfx),
    #[cfg(feature = "backend-drm")]
    Drm(DrmGfx),
}

impl Gfx {
    /// Compose one frame on every output. The flag is true when the frame was queued for a
    /// page flip: the scene engine then learns of it only when the flip completes.
    pub fn render(&mut self, state: &State, plan: &Plan, now_ms: u64) -> Result<(Vec<CapturedFrame>, bool)> {
        match self {
            Gfx::Headless(g) => g.render(state, plan, now_ms).map(|f| (f, false)),
            #[cfg(feature = "backend-winit")]
            Gfx::Winit(g) => g.render(state, plan, now_ms).map(|f| (f, false)),
            #[cfg(feature = "backend-drm")]
            Gfx::Drm(g) => g.render(state, plan, now_ms),
        }
    }

    pub fn import_dmabuf(&mut self, dmabuf: &Dmabuf) -> bool {
        match self {
            Gfx::Headless(g) => g.renderer.import_dmabuf(dmabuf, None).is_ok(),
            #[cfg(feature = "backend-winit")]
            Gfx::Winit(g) => g.import_dmabuf(dmabuf),
            #[cfg(feature = "backend-drm")]
            Gfx::Drm(g) => g.import_dmabuf(dmabuf),
        }
    }

    pub fn dmabuf_formats(&mut self) -> FormatSet {
        match self {
            Gfx::Headless(g) => g.renderer.dmabuf_formats(),
            #[cfg(feature = "backend-winit")]
            Gfx::Winit(g) => g.dmabuf_formats(),
            #[cfg(feature = "backend-drm")]
            Gfx::Drm(g) => g.dmabuf_formats(),
        }
    }

    pub fn output_added(&mut self, info: &OutputInfo) {
        match self {
            Gfx::Headless(g) => g.ensure_output(info),
            #[cfg(feature = "backend-winit")]
            Gfx::Winit(_) => {}
            #[cfg(feature = "backend-drm")]
            Gfx::Drm(_) => {}
        }
    }

    pub fn output_changed(&mut self, info: &OutputInfo) {
        self.output_added(info);
    }

    pub fn set_dpms(&mut self, on: bool) {
        match self {
            Gfx::Headless(_) => {}
            #[cfg(feature = "backend-winit")]
            Gfx::Winit(_) => {}
            #[cfg(feature = "backend-drm")]
            Gfx::Drm(g) => g.set_dpms(on),
        }
        let _ = on;
    }
}

pub(crate) struct HeadOut {
    image: pixman::Image<'static, 'static>,
    tracker: OutputDamageTracker,
    size: (u32, u32),
    scale_120: u32,
}

pub(crate) struct HeadlessGfx {
    renderer: PixmanRenderer,
    outs: HashMap<String, HeadOut>,
    cursor: Option<MemoryRenderBuffer>,
}

impl HeadlessGfx {
    fn ensure_output(&mut self, info: &OutputInfo) {
        let stale = self
            .outs
            .get(&info.name)
            .is_none_or(|o| o.size != (info.width, info.height) || o.scale_120 != info.scale_120);
        if !stale {
            return;
        }
        let size: Size<i32, smithay::utils::Buffer> = (info.width as i32, info.height as i32).into();
        let image = self
            .renderer
            .create_buffer(smithay::backend::allocator::Fourcc::Argb8888, size)
            .expect("pixman image");
        let tracker = OutputDamageTracker::new(
            (info.width as i32, info.height as i32),
            info.scale_120.max(1) as f64 / 120.0,
            Transform::Normal,
        );
        self.outs.insert(
            info.name.clone(),
            HeadOut { image, tracker, size: (info.width, info.height), scale_120: info.scale_120 },
        );
    }

    fn render(&mut self, state: &State, plan: &Plan, now_ms: u64) -> Result<Vec<CapturedFrame>> {
        let mut frames = Vec::new();
        self.outs.retain(|name, _| state.outputs.iter().any(|o| &o.info.name == name));
        for rec in &state.outputs {
            self.ensure_output(&rec.info);
            let out = self.outs.get_mut(&rec.info.name).expect("ensured");
            let elements = build_elements(state, &mut self.renderer, rec, plan, self.cursor.as_ref(), state.frame_seq);
            {
                let mut target = self.renderer.bind(&mut out.image).map_err(|e| anyhow::anyhow!("bind: {e:?}"))?;
                let c = state.cover;
                out.tracker
                    .render_output(&mut self.renderer, &mut target, 0, &elements, Color32F::new(c.r, c.g, c.b, 1.0))
                    .map_err(|e| anyhow::anyhow!("render_output: {e:?}"))?;
            }
            let (w, h) = out.size;
            let stride = out.image.stride();
            // SAFETY: the image is w*h Argb8888 pixels with `stride` bytes per row and
            // nothing else touches it while we hold &mut self.
            let raw = unsafe { std::slice::from_raw_parts(out.image.data() as *const u8, stride * h as usize) };
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h as usize {
                let row = &raw[y * stride..y * stride + w as usize * 4];
                for px in row.chunks_exact(4) {
                    // Argb8888 little endian: B, G, R, A.
                    rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
                }
            }
            let frame = CapturedFrame {
                output: rec.info.name.clone(),
                seq: state.frame_seq,
                time_ms: now_ms,
                width: w,
                height: h,
                rgba,
                plan: plan.clone(),
            };
            if let Some(dir) = &state.dump_dir {
                let _ = std::fs::create_dir_all(dir);
                let path = dir.join(format!("frame-{:05}-{}.png", frame.seq, frame.output));
                if let Err(e) = frame.save_png(&path) {
                    tracing::warn!("frame dump failed: {e:#}");
                }
            }
            frames.push(frame);
        }
        Ok(frames)
    }
}

pub(crate) fn init_headless(
    state: &mut State,
    outputs: Vec<OutputInfo>,
    drive: HeadlessDrive,
    draw_cursor: bool,
    _ctl: &mut Option<()>,
) -> Result<()> {
    let renderer = PixmanRenderer::new().map_err(|e| anyhow::anyhow!("pixman: {e:?}"))?;
    state.gfx = Some(Gfx::Headless(HeadlessGfx {
        renderer,
        outs: HashMap::new(),
        cursor: draw_cursor.then(default_cursor_buffer),
    }));
    state.set_outputs(outputs);
    match drive {
        HeadlessDrive::Wall => {
            state
                .handle
                .insert_source(Timer::from_duration(Duration::from_millis(16)), |_, _, state| {
                    state.render_frame();
                    TimeoutAction::ToDuration(Duration::from_millis(16))
                })
                .map_err(|e| anyhow::anyhow!("frame timer: {e}"))?;
        }
        HeadlessDrive::Stepped(rx) => {
            state
                .handle
                .insert_source(rx, |ev, _, state| {
                    if let ChannelEvent::Msg(cmd) = ev {
                        state.handle_harness(cmd);
                    } else {
                        state.stopped = true;
                    }
                })
                .map_err(|e| anyhow::anyhow!("harness channel: {e}"))?;
        }
    }
    Ok(())
}

impl State {
    pub(crate) fn handle_harness(&mut self, cmd: HarnessCmd) {
        match cmd {
            HarnessCmd::Scene(c) => self.handle_command(c),
            HarnessCmd::Step { dt_ms, reply } => {
                if let Clock::Manual(ms) = &mut self.clock {
                    *ms += dt_ms;
                }
                self.render_frame();
                self.flush();
                let _ = reply.send(std::mem::take(&mut self.harness_frames));
            }
            HarnessCmd::Barrier(tx) => {
                self.flush();
                let _ = tx.send(());
            }
            HarnessCmd::SetOutputs(o) => self.set_outputs(o),
            HarnessCmd::Key { code, pressed } => {
                let t = self.now_ms() as u32;
                self.key(code, pressed, t);
            }
            HarnessCmd::PointerTo { x, y } => {
                let t = self.now_ms() as u32;
                self.pointer_moved_to(x, y, t);
            }
            HarnessCmd::PointerButton { code, pressed } => {
                let t = self.now_ms() as u32;
                self.pointer_button(code, pressed, t);
            }
            HarnessCmd::Focus(tx) => {
                let classify = |s: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface| {
                    let mut root = s.clone();
                    while let Some(p) = smithay::wayland::compositor::get_parent(&root) {
                        root = p;
                    }
                    match self.surface_index.get(&root) {
                        Some((sid, _)) => FocusTarget::Session(sid.clone()),
                        None => FocusTarget::Greeter,
                    }
                };
                let snap = FocusSnapshot {
                    owner: self.owner.clone(),
                    keyboard: self.keyboard.current_focus().as_ref().map(&classify),
                    pointer: self.pointer.current_focus().as_ref().map(&classify),
                };
                let _ = tx.send(snap);
            }
            HarnessCmd::Stop => self.stopped = true,
        }
    }

    pub(crate) fn create_dmabuf_global(&mut self) {
        let formats = self.gfx.as_mut().map(|g| g.dmabuf_formats()).unwrap_or_default();
        let g = self.dmabuf_state.create_global_with_filter::<State, _>(&self.dh, formats, |c| {
            crate::roles::client_may_see(c, GlobalKind::LinuxDmabuf)
        });
        self.dmabuf_global = Some(g);
    }

    pub(crate) fn loop_timeout(&self) -> Option<Duration> {
        Some(Duration::from_millis(250))
    }

    pub(crate) fn after_dispatch(&mut self) {
        self.expire_pending();
        if let Some(a) = &self.awaiting {
            if a.since.elapsed() > Duration::from_millis(500) {
                tracing::warn!("page flip did not complete in 500 ms; releasing the scene engine");
                self.complete_awaiting();
            }
        }
        if self.backend == crate::Backend::Drm && self.needs_redraw && self.awaiting.is_none() {
            self.render_frame();
        }
        self.flush();
    }

    pub(crate) fn shutdown(&mut self) {
        let ids: Vec<_> = self.sessions.keys().cloned().collect();
        for sid in ids {
            let g = self.sessions[&sid].generation;
            self.unregister_session(&sid, g);
        }
        let _ = std::fs::remove_file(&self.cfg.greeter_socket);
        if let Some(t) = self.greeter_listener.take() {
            self.handle.remove(t);
        }
    }
}
