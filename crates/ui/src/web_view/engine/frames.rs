//! The engine's frames, handed to a tab's [`PageSurface`] without a copy.
//!
//! A [`vv::Frame`] is a read-only mapping of the engine's shared memory;
//! [`FrameBytes`] lends it to the frame view as its bytes and gives it back
//! (`vv_session_release_frame`) when the last share of it is dropped, which is
//! when the view has drawn the next one.

use std::sync::Arc;

use crate::frame_view::DamageRect;
use vivarium as vv;

use super::state::Entry;
use crate::web_view::surface::PageSurface;

/// The pixels of one engine frame, as the frame view reads them.
pub(super) struct FrameBytes(pub vv::Frame);

impl AsRef<[u8]> for FrameBytes {
    fn as_ref(&self) -> &[u8] {
        // A frame that is not shared memory was refused before it got here.
        self.0.pixels().map_or(&[], |pixels| pixels.bytes())
    }
}

/// What changed since the frame the view shows: everything for a new buffer
/// set (`None` is the whole frame), else the engine's damage clamped to the
/// frame, empty when nothing changed.
pub(super) fn damage_of(frame: &vv::Frame) -> Option<DamageRect> {
    if frame.is_new_buffers() {
        return None;
    }
    let damage = frame.damage();
    Some(if damage.is_empty() {
        DamageRect::new(0, 0, 0, 0)
    } else {
        DamageRect::new(damage.x.max(0) as u32, damage.y.max(0) as u32, damage.width.max(0) as u32, damage.height.max(0) as u32)
            .clamped(frame.width(), frame.height())
    })
}

/// Hand the newest frames of `entry` to its surface.
pub(super) fn take(entry: &Entry) {
    // The event says one is there; after it, more may have come.
    while let Ok(Some(frame)) = entry.session.acquire_frame() {
        let (width, height, stride, scale) = (frame.width(), frame.height(), frame.stride(), frame.scale());
        let (damage, opaque, resize) = (damage_of(&frame), frame.is_opaque(), frame.resize_id().get());
        // Only shared memory was asked for; anything else cannot be read here.
        if frame.pixels().is_none() {
            continue;
        }
        if let Some(mut made) = PageSurface::frame_of(width, height, stride as usize, scale, opaque, damage, Arc::new(FrameBytes(frame))) {
            // The id of the size report this frame answers; 0 is "none yet".
            made.resize_id = (resize != 0).then_some(u64::from(resize));
            entry.surface.push(made);
        }
    }
}
