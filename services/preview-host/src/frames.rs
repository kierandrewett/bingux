//! The slots a video frame travels in.
//!
//! The host keeps up to [`SLOTS`] sealed memory regions, each big enough for a
//! frame of the size the viewer asked for. A frame is copied into a free slot
//! and announced with `FrameReady`; the slot is the viewer's until it answers
//! `FrameDone`. With no free slot the frame is dropped: the clock keeps
//! running and the picture skips, which is what a player under load does.
//!
//! A slot's descriptor travels once, with the first `FrameReady` that uses it
//! (and again when the viewer asks for a larger size and the region is
//! replaced).

use std::sync::{Arc, Mutex};

use bingux_preview_core::host::protocol::Message;
use bingux_preview_core::host::shm::{MAX_BYTES, Shared};

use crate::link::Link;

pub const SLOTS: usize = 3;

struct Slot {
    shared: Option<Shared>,
    capacity: usize,
    /// The viewer has not answered `FrameDone` yet.
    busy: bool,
    /// The viewer has the current region's descriptor.
    announced: bool,
}

struct State {
    slots: [Slot; SLOTS],
    capacity: usize,
    dropped: u64,
    published: u64,
}

pub struct Frames {
    state: Mutex<State>,
}

/// The most bytes a frame of `max_w` by `max_h` takes; the shared region is this big.
pub fn capacity_of(max_w: u32, max_h: u32) -> usize {
    (max_w as usize).saturating_mul(max_h as usize).saturating_mul(4).clamp(4, MAX_BYTES)
}

impl Frames {
    pub fn new(max_w: u32, max_h: u32) -> Arc<Frames> {
        let slot = || Slot { shared: None, capacity: 0, busy: false, announced: false };
        Arc::new(Frames {
            state: Mutex::new(State {
                slots: [slot(), slot(), slot()],
                capacity: capacity_of(max_w, max_h),
                dropped: 0,
                published: 0,
            }),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The viewer wants frames within `max_w` by `max_h` from now on. Regions
    /// are replaced as their slots come free.
    pub fn resize(&self, max_w: u32, max_h: u32) {
        self.lock().capacity = capacity_of(max_w, max_h);
    }

    /// The viewer has copied the frame in `slot`.
    pub fn done(&self, slot: u8) {
        if let Some(slot) = self.lock().slots.get_mut(slot as usize) {
            slot.busy = false;
        }
    }

    /// How many frames were sent and how many were dropped for want of a slot.
    pub fn counts(&self) -> (u64, u64) {
        let state = self.lock();
        (state.published, state.dropped)
    }

    /// Copy a frame of `width` by `height` into a free slot and announce it.
    /// `rows` yields the frame's `height` rows, each at least `width * 4`
    /// bytes. A frame that does not fit the capacity is dropped.
    pub fn publish<'a>(&self, link: &Link, width: u32, height: u32, pts_ns: u64, rows: impl Iterator<Item = &'a [u8]>) {
        let size = (width as usize).saturating_mul(height as usize).saturating_mul(4);
        let mut guard = self.lock();
        let state = &mut *guard;
        if size == 0 || size > state.capacity {
            state.dropped += 1;
            return;
        }
        let capacity = state.capacity;
        let Some(index) = state.slots.iter().position(|slot| !slot.busy) else {
            state.dropped += 1;
            return;
        };
        let slot = &mut state.slots[index];
        if slot.shared.is_none() || slot.capacity != capacity {
            match Shared::create("bingux-frame", capacity) {
                Ok(shared) => {
                    slot.shared = Some(shared);
                    slot.capacity = capacity;
                    slot.announced = false;
                }
                Err(error) => {
                    eprintln!("bingux-preview-host: no memory for a frame: {error}");
                    state.dropped += 1;
                    return;
                }
            }
        }
        let slot = &mut state.slots[index];
        let Some(shared) = slot.shared.as_mut() else { return };
        let region = shared.bytes_mut();
        let row_bytes = width as usize * 4;
        for (row, line) in rows.take(height as usize).enumerate() {
            let Some(source) = line.get(..row_bytes) else { break };
            region[row * row_bytes..(row + 1) * row_bytes].copy_from_slice(source);
        }
        slot.busy = true;
        let first_use = !slot.announced;
        slot.announced = true;
        let message = Message::FrameReady {
            slot: index as u8,
            width,
            height,
            stride: row_bytes as u32,
            size: size as u32,
            pts_ns,
        };
        let fd = first_use.then(|| shared.raw_fd());
        state.published += 1;
        // Sent under the lock so a `FrameDone` for this slot cannot overtake
        // the announcement of its region.
        match fd {
            Some(fd) => link.send_with(0, message, &[fd]),
            None => link.send(0, message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bingux_preview_core::host::channel::Channel;
    use bingux_preview_core::host::protocol::Message;

    fn rows(width: u32, height: u32, value: u8) -> Vec<Vec<u8>> {
        (0..height).map(|_| vec![value; width as usize * 4]).collect()
    }

    fn frame(frames: &Frames, link: &Link, width: u32, height: u32, value: u8) {
        let data = rows(width, height, value);
        frames.publish(link, width, height, 0, data.iter().map(|row| row.as_slice()));
    }

    #[test]
    fn frames_fill_free_slots_and_the_fourth_is_dropped() {
        let (host, viewer) = Channel::pair().unwrap();
        let link = Link::new(host);
        let frames = Frames::new(16, 16);
        for value in 1..=4 {
            frame(&frames, &link, 8, 8, value);
        }
        assert_eq!(frames.counts(), (3, 1));
        let mut seen = Vec::new();
        for _ in 0..3 {
            let received = viewer.recv().unwrap();
            let Message::FrameReady { slot, width, size, .. } = received.packet.message else { panic!("a frame") };
            assert_eq!((width, size), (8, 256));
            assert_eq!(received.fds.len(), 1, "the first use of a slot carries its memory");
            seen.push(slot);
        }
        seen.sort_unstable();
        assert_eq!(seen, vec![0, 1, 2]);
        // Done frees a slot, and the region is not sent again.
        frames.done(1);
        frame(&frames, &link, 8, 8, 9);
        let received = viewer.recv().unwrap();
        assert!(matches!(received.packet.message, Message::FrameReady { slot: 1, .. }));
        assert!(received.fds.is_empty());
    }

    #[test]
    fn what_the_viewer_maps_is_what_the_host_wrote() {
        let (host, viewer) = Channel::pair().unwrap();
        let link = Link::new(host);
        let frames = Frames::new(4, 2);
        frame(&frames, &link, 4, 2, 7);
        let mut received = viewer.recv().unwrap();
        let Message::FrameReady { size, .. } = received.packet.message else { panic!("a frame") };
        let shared = Shared::map(received.fds.remove(0), size as usize).unwrap();
        assert_eq!(shared.copy_to(size as usize), vec![7u8; 32]);
    }

    #[test]
    fn a_frame_larger_than_the_capacity_is_dropped_and_a_resize_replaces_regions() {
        let (host, viewer) = Channel::pair().unwrap();
        let link = Link::new(host);
        let frames = Frames::new(4, 4);
        frame(&frames, &link, 8, 8, 1);
        assert_eq!(frames.counts(), (0, 1));
        frames.resize(8, 8);
        frame(&frames, &link, 8, 8, 1);
        let received = viewer.recv().unwrap();
        assert_eq!(received.fds.len(), 1);
        frames.done(0);
        frames.resize(16, 16);
        frame(&frames, &link, 12, 12, 1);
        let received = viewer.recv().unwrap();
        assert_eq!(received.fds.len(), 1, "a bigger region has a new descriptor");
    }
}
