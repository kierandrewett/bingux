//! Development backend: one window in an existing Wayland desktop. The window is a
//! single output named `WL-1` whose size follows the window.

use anyhow::Result;
use bsm_proto::scene::OutputInfo;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::format::FormatSet;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{Color32F, ImportDma};
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::utils::Transform;

use crate::engine::Plan;
use crate::gfx::Gfx;
use crate::harness::CapturedFrame;
use crate::render::{build_elements, default_cursor_buffer};
use crate::state::State;

const OUTPUT: &str = "WL-1";

pub(crate) struct WinitGfx {
    backend: WinitGraphicsBackend<GlesRenderer>,
    tracker: OutputDamageTracker,
    cursor: MemoryRenderBuffer,
}

impl WinitGfx {
    pub fn render(&mut self, state: &State, plan: &Plan, _now_ms: u64) -> Result<Vec<CapturedFrame>> {
        let Some(rec) = state.outputs.first() else { return Ok(Vec::new()) };
        let age = self.backend.buffer_age().unwrap_or(0);
        let damage = {
            let (renderer, mut fb) = self.backend.bind().map_err(|e| anyhow::anyhow!("bind: {e:?}"))?;
            let elements = build_elements(state, renderer, rec, plan, Some(&self.cursor), state.frame_seq);
            let c = state.cover;
            let res = self
                .tracker
                .render_output(renderer, &mut fb, age, &elements, Color32F::new(c.r, c.g, c.b, 1.0))
                .map_err(|e| anyhow::anyhow!("render_output: {e:?}"))?;
            res.damage.cloned()
        };
        self.backend
            .submit(damage.as_deref())
            .map_err(|e| anyhow::anyhow!("submit: {e:?}"))?;
        Ok(Vec::new())
    }

    pub fn import_dmabuf(&mut self, d: &Dmabuf) -> bool {
        self.backend.renderer().import_dmabuf(d, None).is_ok()
    }

    pub fn dmabuf_formats(&mut self) -> FormatSet {
        self.backend.renderer().dmabuf_formats()
    }

    fn resize(&mut self, w: i32, h: i32) {
        self.tracker = OutputDamageTracker::new((w, h), 1.0, Transform::Flipped180);
    }
}

fn info_for(w: i32, h: i32) -> OutputInfo {
    OutputInfo {
        name: OUTPUT.into(),
        x: 0,
        y: 0,
        width: w as u32,
        height: h as u32,
        scale_120: 120,
        refresh_mhz: 60_000,
        transform: 0,
    }
}

pub(crate) fn init_winit(state: &mut State) -> Result<()> {
    let (backend, event_loop) =
        winit::init::<GlesRenderer>().map_err(|e| anyhow::anyhow!("winit backend: {e:?}"))?;
    backend.window().set_title("bsm (development)");
    let size = backend.window_size();
    let tracker = OutputDamageTracker::new((size.w, size.h), 1.0, Transform::Flipped180);
    state.gfx = Some(Gfx::Winit(WinitGfx { backend, tracker, cursor: default_cursor_buffer() }));
    state.set_outputs(vec![info_for(size.w, size.h)]);

    state
        .handle
        .insert_source(event_loop, |event, _, state| match event {
            WinitEvent::Resized { size, .. } => {
                if let Some(Gfx::Winit(g)) = state.gfx.as_mut() {
                    g.resize(size.w, size.h);
                }
                state.set_outputs(vec![info_for(size.w, size.h)]);
            }
            WinitEvent::Input(ev) => {
                let _ = state.process_input_event(ev, false);
            }
            WinitEvent::CloseRequested => state.stopped = true,
            WinitEvent::Redraw => {
                state.render_frame();
                if let Some(Gfx::Winit(g)) = state.gfx.as_ref() {
                    g.backend.window().request_redraw();
                }
            }
            WinitEvent::Focus(_) => {}
        })
        .map_err(|e| anyhow::anyhow!("winit source: {e}"))?;
    if let Some(Gfx::Winit(g)) = state.gfx.as_ref() {
        g.backend.window().request_redraw();
    }
    Ok(())
}
