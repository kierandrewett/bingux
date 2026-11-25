//! The engine's frames, handed to a tab's [`PageSurface`].
//!
//! A [`Frame`] is the engine's shared memory; its bytes are read through the
//! crate's checked view, which copies the mapping once per frame (the engine
//! is untrusted and could write a buffer it handed over; a copy of the host's
//! own cannot tear under a reader). [`FrameBytes`] lends that copy to the
//! frame view and gives the frame back (`vv_session_release_frame`) when the
//! last share of it is dropped, which is when the view has drawn the next one.

use std::sync::Arc;

use vivarium::display::{DamageRect as EngineDamage, Frame, FrameFlags};

use super::state::Entry;
use crate::frame_view::DamageRect;
use crate::web_view::surface::PageSurface;

/// The pixels of one engine frame, as the frame view reads them.
pub(super) struct FrameBytes(pub Frame);

impl AsRef<[u8]> for FrameBytes {
    fn as_ref(&self) -> &[u8] {
        // Empty for a frame that is not shared memory; those are refused before they get here.
        self.0.as_ref()
    }
}

/// What changed since the frame the view shows: everything for a new buffer
/// set (`None` is the whole frame), else the engine's damage clamped to the
/// frame, empty when nothing changed.
pub(super) fn damage_of(
    new_buffers: bool,
    damage: EngineDamage,
    width: u32,
    height: u32,
) -> Option<DamageRect> {
    if new_buffers {
        return None;
    }
    Some(if damage.is_empty() {
        DamageRect::new(0, 0, 0, 0)
    } else {
        DamageRect::new(
            damage.x.max(0) as u32,
            damage.y.max(0) as u32,
            damage.width.max(0) as u32,
            damage.height.max(0) as u32,
        )
        .clamped(width, height)
    })
}

/// Hand the newest frames of `entry` to its surface.
pub(super) fn take(entry: &Entry) {
    // The event says one is there; after it, more may have come.
    while let Ok(Some(frame)) = entry.session.acquire_frame() {
        // Only shared memory was asked for; anything else cannot be read here.
        let Some(stride) = frame.content().shm().map(|shm| shm.stride()) else {
            continue;
        };
        let (width, height, scale) = (frame.width(), frame.height(), frame.scale());
        let damage = damage_of(frame.is_new_buffers(), frame.damage(), width, height);
        let opaque = !frame.flags().contains(FrameFlags::TRANSPARENT);
        let resize = frame.resize_id().get();
        if let Some(mut made) = PageSurface::frame_of(
            width,
            height,
            stride as usize,
            scale,
            opaque,
            damage,
            Arc::new(FrameBytes(frame)),
        ) {
            // The id of the size report this frame answers; 0 is "none yet".
            made.resize_id = (resize != 0).then_some(u64::from(resize));
            entry.surface.push(made);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_buffer_set_is_drawn_whole_and_damage_stays_inside_the_frame() {
        let none = EngineDamage {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        assert_eq!(
            damage_of(true, none, 100, 50),
            None,
            "a new buffer set is the whole frame"
        );
        assert_eq!(
            damage_of(false, none, 100, 50),
            Some(DamageRect::new(0, 0, 0, 0)),
            "nothing changed"
        );
        let past = EngineDamage {
            x: -5,
            y: 40,
            width: 200,
            height: 20,
        };
        assert_eq!(
            damage_of(false, past, 100, 50),
            Some(DamageRect::new(0, 40, 200, 20).clamped(100, 50))
        );
    }
}
