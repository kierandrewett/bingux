//! Turns a [`Plan`] into render elements for one output. Generic over the renderer, so
//! the Pixman (headless) and GLES (winit, DRM) backends compose identically.
//!
//! Layer order, top to bottom: cursor, cover-colour overlay (group fade), then the
//! session and the greeter in the order the plan gives. The cover colour itself is
//! the clear colour, so no frame ever has an undefined pixel.
//!
//! The session is drawn from its root surface only, never its subsurface tree, and at
//! one buffer pixel per output pixel regardless of the output's fractional scale.

use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement};
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree};
use smithay::backend::renderer::element::utils::{CropRenderElement, RescaleRenderElement};
use smithay::backend::renderer::element::Id;
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{Color32F, ImportAll, ImportMem, Renderer, Texture};
use smithay::desktop::layer_map_for_output;
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::render_elements;
use smithay::utils::{Physical, Point, Rectangle, Scale, Transform};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::wlr_layer::Layer;

use crate::engine::Plan;
use crate::state::{OutputRec, State};

render_elements! {
    pub SceneElement<R> where R: ImportAll + ImportMem;
    Surface = WaylandSurfaceRenderElement<R>,
    Rescaled = RescaleRenderElement<WaylandSurfaceRenderElement<R>>,
    Cropped = CropRenderElement<RescaleRenderElement<WaylandSurfaceRenderElement<R>>>,
    Solid = SolidColorRenderElement,
    Memory = MemoryRenderBufferRenderElement<R>,
}

/// Persistent identity of the cover overlay element per output (damage tracking).
pub(crate) fn overlay_id() -> Id {
    thread_local! { static ID: Id = Id::new(); }
    ID.with(|i| i.clone())
}

/// The built-in arrow used when the owner asks for a named or default cursor.
pub(crate) fn default_cursor_buffer() -> MemoryRenderBuffer {
    const ART: [&str; 19] = [
        "#           ",
        "##          ",
        "#.#         ",
        "#..#        ",
        "#...#       ",
        "#....#      ",
        "#.....#     ",
        "#......#    ",
        "#.......#   ",
        "#........#  ",
        "#.........# ",
        "#......#####",
        "#...#..#    ",
        "#..# #..#   ",
        "#.#  #..#   ",
        "##    #..#  ",
        "#     #..#  ",
        "       ##   ",
        "            ",
    ];
    let (w, h) = (12usize, ART.len());
    let mut px = vec![0u8; w * h * 4];
    for (y, row) in ART.iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            let v: [u8; 4] = match c {
                b'#' => [0, 0, 0, 255],
                b'.' => [255, 255, 255, 255],
                _ => [0, 0, 0, 0],
            };
            // Argb8888 little endian is B, G, R, A in memory.
            let o = (y * w + x) * 4;
            px[o] = v[2];
            px[o + 1] = v[1];
            px[o + 2] = v[0];
            px[o + 3] = v[3];
        }
    }
    MemoryRenderBuffer::from_slice(
        &px,
        smithay::backend::allocator::Fourcc::Argb8888,
        (w as i32, h as i32),
        1,
        Transform::Normal,
        None,
    )
}

/// Rectangle of the output in its own physical pixels.
fn output_rect(rec: &OutputRec) -> Rectangle<i32, Physical> {
    let size = rec.output.current_mode().map(|m| m.size).unwrap_or_default();
    Rectangle::from_size(size)
}

pub(crate) fn build_elements<R>(
    state: &State,
    renderer: &mut R,
    rec: &OutputRec,
    plan: &Plan,
    cursor: Option<&MemoryRenderBuffer>,
    seq: u64,
) -> Vec<SceneElement<R>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Texture + Clone + Send + 'static,
    R::Error: std::fmt::Debug,
{
    let mut out: Vec<SceneElement<R>> = Vec::new();
    let rect = output_rect(rec);
    let scale = rec.output.current_scale().fractional_scale();
    let tscale = Scale::from(scale);

    // Cursor on top, only while somebody owns the seat and the pointer is on this output.
    if let Some(c) = cursor_elements(state, renderer, rec, cursor, tscale) {
        out.extend(c);
    }

    if plan.cover_overlay > 0.0 {
        let a = plan.cover_overlay.clamp(0.0, 1.0);
        let c = state.cover;
        out.push(SceneElement::Solid(SolidColorRenderElement::new(
            overlay_id(),
            rect,
            CommitCounter::from(seq as usize),
            Color32F::new(c.r * a, c.g * a, c.b * a, a),
            Kind::Unspecified,
        )));
    }

    let greeter = |renderer: &mut R| -> Vec<SceneElement<R>> {
        let Some(layer) = &plan.greeter else { return Vec::new() };
        let shift = (layer.offset_y as f64 * rect.size.h as f64).round() as i32;
        let map = layer_map_for_output(&rec.output);
        let mut v = Vec::new();
        for l in [Layer::Overlay, Layer::Top, Layer::Bottom, Layer::Background] {
            for surf in map.layers_on(l).rev() {
                let Some(geo) = map.layer_geometry(surf) else { continue };
                let loc: Point<i32, Physical> = geo.loc.to_physical_precise_round(tscale);
                let loc = Point::from((loc.x, loc.y + shift));
                v.extend(render_elements_from_surface_tree::<R, SceneElement<R>>(
                    renderer,
                    surf.wl_surface(),
                    loc,
                    tscale,
                    1.0,
                    Kind::Unspecified,
                ));
            }
        }
        v
    };

    let session = |renderer: &mut R| -> Vec<SceneElement<R>> {
        let Some(layer) = &plan.session else { return Vec::new() };
        let Some(s) = state.sessions.get(&layer.key.0).filter(|s| s.generation == layer.key.1) else {
            return Vec::new();
        };
        let Some(hs) = s.surfaces.get(&rec.info.name) else { return Vec::new() };
        let elem = with_states(&hs.surface, |states| {
            WaylandSurfaceRenderElement::<R>::from_surface(
                renderer,
                &hs.surface,
                states,
                Point::<f64, Physical>::from((0.0, 0.0)),
                layer.opacity,
                Kind::Unspecified,
            )
        });
        let Ok(Some(elem)) = elem else { return Vec::new() };
        // One buffer pixel per output pixel: undo the output scale the tracker applies.
        let elem = RescaleRenderElement::from_element(elem, Point::from((0, 0)), Scale::from(1.0 / scale));
        if layer.only_vacated {
            let shift = plan
                .greeter
                .map(|g| (-(g.offset_y as f64) * rect.size.h as f64).round() as i32)
                .unwrap_or(rect.size.h)
                .clamp(0, rect.size.h);
            if shift == 0 {
                return Vec::new();
            }
            let crop = Rectangle::new((0, rect.size.h - shift).into(), (rect.size.w, shift).into());
            match CropRenderElement::from_element(elem, tscale, crop) {
                Some(c) => vec![SceneElement::Cropped(c)],
                None => Vec::new(),
            }
        } else {
            vec![SceneElement::Rescaled(elem)]
        }
    };

    if plan.session_above_greeter {
        out.extend(session(renderer));
        out.extend(greeter(renderer));
    } else {
        out.extend(greeter(renderer));
        out.extend(session(renderer));
    }
    out
}

fn cursor_elements<R>(
    state: &State,
    renderer: &mut R,
    rec: &OutputRec,
    default_cursor: Option<&MemoryRenderBuffer>,
    tscale: Scale<f64>,
) -> Option<Vec<SceneElement<R>>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Texture + Clone + Send + 'static,
    R::Error: std::fmt::Debug,
{
    use bsm_proto::state::InputOwner;
    if state.owner == InputOwner::None {
        return None;
    }
    let o = rec.output.current_location();
    let local = state.pointer_pos - o.to_f64();
    let logical_size = rec
        .output
        .current_mode()
        .map(|m| m.size.to_f64().to_logical(rec.output.current_scale().fractional_scale()))?;
    if local.x < 0.0 || local.y < 0.0 || local.x >= logical_size.w || local.y >= logical_size.h {
        return None;
    }
    let pos: Point<f64, Physical> = local.to_physical(tscale);
    match &state.cursor {
        CursorImageStatus::Hidden => None,
        CursorImageStatus::Surface(s) if !cursor_surface_ok(s) => {
            // A "cursor" the size of a window is an overlay, not a cursor.
            let buf = default_cursor?;
            let e = MemoryRenderBufferRenderElement::from_buffer(renderer, pos, buf, None, None, None, Kind::Cursor)
                .ok()?;
            Some(vec![SceneElement::Memory(e)])
        }
        CursorImageStatus::Surface(s) => {
            let hot = with_states(s, |st| {
                st.data_map
                    .get::<CursorImageSurfaceData>()
                    .map(|d| d.lock().unwrap().hotspot)
                    .unwrap_or_default()
            });
            let p: Point<i32, Physical> = (pos - hot.to_f64().to_physical(tscale)).to_i32_round();
            Some(render_elements_from_surface_tree::<R, SceneElement<R>>(
                renderer, s, p, tscale, 1.0, Kind::Cursor,
            ))
        }
        CursorImageStatus::Named(_) => {
            let buf = default_cursor?;
            let e = MemoryRenderBufferRenderElement::from_buffer(renderer, pos, buf, None, None, None, Kind::Cursor)
                .ok()?;
            Some(vec![SceneElement::Memory(e)])
        }
    }
}

/// Cursor surfaces larger than this are replaced by the built-in arrow.
const MAX_CURSOR_PX: i32 = 256;

fn cursor_surface_ok(s: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface) -> bool {
    smithay::backend::renderer::utils::with_renderer_surface_state(s, |st| st.buffer_size())
        .flatten()
        .is_none_or(|sz| sz.w <= MAX_CURSOR_PX && sz.h <= MAX_CURSOR_PX)
}
