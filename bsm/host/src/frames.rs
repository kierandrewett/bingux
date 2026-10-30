//! Shared-memory frame buffers and the per-output gate between the
//! PipeWire thread and the Wayland thread.
//!
//! The PipeWire thread copies a frame into a free slot of the current
//! [`BufferSet`]; the main thread attaches and commits that slot's `wl_buffer`
//! and marks the slot free again when the compositor releases it. A slot is
//! written only by the thread that claimed it, and claimed slots are never
//! handed to the compositor until the copy is finished, so no slot is read and
//! written at the same time.

use std::os::fd::AsFd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use memmap2::MmapMut;
use rustix::fs::{MemfdFlags, ftruncate, memfd_create};
use wayland_client::protocol::{wl_buffer, wl_shm, wl_shm_pool};
use wayland_client::{Dispatch, QueueHandle};

/// Slots per output. Four absorbs a compositor holding two buffers plus one in
/// copy plus one in flight without stalling the stream.
pub const SLOTS: usize = 4;
/// Bytes per pixel of `wl_shm.format.xrgb8888`.
pub const BPP: usize = 4;

/// User data of every `wl_buffer` the bridge creates.
#[derive(Debug, Clone)]
pub struct BufferTag {
    pub output: String,
    pub set_id: u64,
    pub slot: usize,
}

struct Slot {
    buffer: wl_buffer::WlBuffer,
    /// Claimed for writing, or committed and not yet released by the compositor.
    busy: AtomicBool,
    /// Holds the newest committed frame so it can be presented again.
    pinned: AtomicBool,
}

/// A set of `SLOTS` equally sized `wl_buffer`s backed by one memfd.
pub struct BufferSet {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    map: MmapMut,
    slots: Vec<Slot>,
    pool: wl_shm_pool::WlShmPool,
}

// SAFETY: `map` is written only through `slot_mut`, whose contract requires the
// caller to hold the slot's `busy` flag; the flag is taken with an atomic swap.
// The wayland proxies are `Send + Sync`.
unsafe impl Send for BufferSet {}
unsafe impl Sync for BufferSet {}

impl BufferSet {
    pub fn new<D>(
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<D>,
        output: &str,
        id: u64,
        width: u32,
        height: u32,
    ) -> anyhow::Result<Arc<Self>>
    where
        D: Dispatch<wl_shm_pool::WlShmPool, ()> + Dispatch<wl_buffer::WlBuffer, BufferTag> + 'static,
    {
        anyhow::ensure!(width > 0 && height > 0, "zero sized buffer set");
        let stride = width as usize * BPP;
        let slot_size = stride
            .checked_mul(height as usize)
            .filter(|s| *s <= i32::MAX as usize / SLOTS)
            .ok_or_else(|| anyhow::anyhow!("buffer {width}x{height} too large"))?;
        let total = slot_size * SLOTS;
        let fd = memfd_create("bsm-host-frames", MemfdFlags::CLOEXEC)?;
        ftruncate(&fd, total as u64)?;
        // SAFETY: the memfd is private to this process until the pool is created
        // and the compositor maps it read-only in practice; we only write
        // through claimed slots.
        let map = unsafe { MmapMut::map_mut(&fd)? };
        let pool = shm.create_pool(fd.as_fd(), total as i32, qh, ());
        let slots = (0..SLOTS)
            .map(|slot| Slot {
                buffer: pool.create_buffer(
                    (slot * slot_size) as i32,
                    width as i32,
                    height as i32,
                    stride as i32,
                    wl_shm::Format::Xrgb8888,
                    qh,
                    BufferTag { output: output.to_owned(), set_id: id, slot },
                ),
                busy: AtomicBool::new(false),
                pinned: AtomicBool::new(false),
            })
            .collect();
        Ok(Arc::new(Self { id, width, height, stride: stride as u32, map, slots, pool }))
    }

    pub fn slot_size(&self) -> usize {
        self.stride as usize * self.height as usize
    }

    /// Claim a free slot for writing. `None` when the compositor holds them all.
    pub fn claim(&self) -> Option<usize> {
        self.slots.iter().position(|s| {
            !s.pinned.load(Ordering::Acquire) && !s.busy.swap(true, Ordering::AcqRel)
        })
    }

    /// Pin `slot` (the frame just committed) and unpin `previous`.
    pub fn pin(&self, slot: usize, previous: Option<usize>) {
        self.slots[slot].pinned.store(true, Ordering::Release);
        if let Some(p) = previous.filter(|p| *p != slot) {
            self.slots[p].pinned.store(false, Ordering::Release);
        }
    }

    /// Mark `slot` free again (compositor released it or the frame was dropped).
    pub fn release(&self, slot: usize) {
        if let Some(s) = self.slots.get(slot) {
            s.busy.store(false, Ordering::Release);
        }
    }

    pub fn is_busy(&self, slot: usize) -> bool {
        self.slots.get(slot).is_some_and(|s| s.busy.load(Ordering::Acquire))
    }

    pub fn buffer(&self, slot: usize) -> &wl_buffer::WlBuffer {
        &self.slots[slot].buffer
    }

    /// Mutable access to a claimed slot.
    ///
    /// # Safety
    /// The caller must own `slot` (obtained from [`claim`](Self::claim) and not
    /// yet handed to the compositor or released).
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn slot_mut(&self, slot: usize) -> &mut [u8] {
        let size = self.slot_size();
        let base = self.map.as_ptr() as *mut u8;
        // SAFETY: in bounds by construction (`slot < SLOTS`); exclusivity is the caller's contract.
        unsafe { std::slice::from_raw_parts_mut(base.add(slot * size), size) }
    }

    /// Read access to a slot that is not being written.
    pub fn slot(&self, slot: usize) -> &[u8] {
        let size = self.slot_size();
        &self.map[slot * size..(slot + 1) * size]
    }

    /// True when no slot is held by the compositor or being written.
    pub fn all_free(&self) -> bool {
        self.slots.iter().all(|s| !s.busy.load(Ordering::Acquire))
    }

    /// Destroy the wayland objects. The slots' memory stays valid until the
    /// last `Arc` drops, which is what the compositor's mapping needs.
    pub fn destroy(&self) {
        for s in &self.slots {
            s.buffer.destroy();
        }
        self.pool.destroy();
    }
}

/// Create a single ARGB8888 `wl_buffer` holding `pixels` (premultiplied BGRA
/// bytes, tightly packed). The pool is destroyed at once; the buffer keeps the
/// mapping alive.
pub fn shm_image<D>(
    shm: &wl_shm::WlShm,
    qh: &QueueHandle<D>,
    width: u32,
    height: u32,
    pixels: &[u8],
    tag: BufferTag,
) -> anyhow::Result<wl_buffer::WlBuffer>
where
    D: Dispatch<wl_shm_pool::WlShmPool, ()> + Dispatch<wl_buffer::WlBuffer, BufferTag> + 'static,
{
    anyhow::ensure!(pixels.len() == width as usize * height as usize * 4 && !pixels.is_empty(), "bad image size");
    let fd = memfd_create("bsm-host-cursor", MemfdFlags::CLOEXEC)?;
    ftruncate(&fd, pixels.len() as u64)?;
    // SAFETY: private memfd, written once before the pool exists.
    let mut map = unsafe { MmapMut::map_mut(&fd)? };
    map.copy_from_slice(pixels);
    let pool = shm.create_pool(fd.as_fd(), pixels.len() as i32, qh, ());
    let buffer = pool.create_buffer(0, width as i32, height as i32, width as i32 * 4, wl_shm::Format::Argb8888, qh, tag);
    pool.destroy();
    Ok(buffer)
}

/// Counters for the cost measurement in `docs/bsm/bridge.md`.
#[derive(Default)]
pub struct Stats {
    pub frames_in: AtomicU64,
    pub frames_copied: AtomicU64,
    pub copy_ns: AtomicU64,
    pub dropped_throttle: AtomicU64,
    pub dropped_no_slot: AtomicU64,
    pub dropped_no_set: AtomicU64,
    pub dropped_size: AtomicU64,
    pub bytes_copied: AtomicU64,
}

impl Stats {
    pub fn summary(&self) -> String {
        let copied = self.frames_copied.load(Ordering::Relaxed);
        let ns = self.copy_ns.load(Ordering::Relaxed);
        let avg_us = if copied > 0 { ns as f64 / copied as f64 / 1000.0 } else { 0.0 };
        format!(
            "frames_in={} copied={} avg_copy_us={avg_us:.0} dropped(throttle={} no_slot={} no_set={} size={})",
            self.frames_in.load(Ordering::Relaxed),
            copied,
            self.dropped_throttle.load(Ordering::Relaxed),
            self.dropped_no_slot.load(Ordering::Relaxed),
            self.dropped_no_set.load(Ordering::Relaxed),
            self.dropped_size.load(Ordering::Relaxed),
        )
    }
}

/// Per-output state shared with the stream thread.
pub struct OutputGate {
    /// The buffer set frames may be written into. `None` until the configure is
    /// acknowledged: nothing is presented before the first ack.
    current: Mutex<Option<Arc<BufferSet>>>,
    /// bsm routes input to this session (foreground).
    active: AtomicBool,
    last_forwarded: Mutex<Option<Instant>>,
    background_interval: Duration,
    pub stats: Stats,
}

impl OutputGate {
    pub fn new(background_interval: Duration) -> Arc<Self> {
        Arc::new(Self {
            current: Mutex::new(None),
            active: AtomicBool::new(false),
            last_forwarded: Mutex::new(None),
            background_interval,
            stats: Stats::default(),
        })
    }

    pub fn set_buffers(&self, set: Option<Arc<BufferSet>>) -> Option<Arc<BufferSet>> {
        std::mem::replace(&mut *self.current.lock().unwrap(), set)
    }

    pub fn buffers(&self) -> Option<Arc<BufferSet>> {
        self.current.lock().unwrap().clone()
    }

    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
        if active {
            *self.last_forwarded.lock().unwrap() = None;
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    /// Decide whether a frame arriving at `now` is forwarded. Foreground
    /// sessions forward every frame; background ones at most once per
    /// `background_interval`.
    pub fn admit(&self, now: Instant) -> bool {
        if self.is_active() {
            return true;
        }
        let mut last = self.last_forwarded.lock().unwrap();
        match *last {
            Some(t) if now.duration_since(t) < self.background_interval => false,
            _ => {
                *last = Some(now);
                true
            }
        }
    }
}

/// Copy `rows` rows of `row_bytes` bytes from `src` (stride `src_stride`) to
/// `dst` (stride `dst_stride`), forcing the X channel to opaque is not needed
/// because the buffers are advertised as XRGB8888.
pub fn copy_rows(
    dst: &mut [u8],
    dst_stride: usize,
    src: &[u8],
    src_stride: usize,
    row_bytes: usize,
    rows: usize,
) {
    if dst_stride == src_stride && row_bytes == dst_stride {
        let n = dst_stride * rows;
        dst[..n].copy_from_slice(&src[..n]);
        return;
    }
    for y in 0..rows {
        dst[y * dst_stride..y * dst_stride + row_bytes]
            .copy_from_slice(&src[y * src_stride..y * src_stride + row_bytes]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_gate_admits_at_most_one_frame_per_interval() {
        let gate = OutputGate::new(Duration::from_millis(100));
        let t0 = Instant::now();
        assert!(gate.admit(t0));
        assert!(!gate.admit(t0 + Duration::from_millis(50)));
        assert!(!gate.admit(t0 + Duration::from_millis(99)));
        assert!(gate.admit(t0 + Duration::from_millis(101)));
    }

    #[test]
    fn foreground_gate_admits_everything_and_reactivation_forces_a_frame() {
        let gate = OutputGate::new(Duration::from_secs(1));
        let t0 = Instant::now();
        assert!(gate.admit(t0));
        assert!(!gate.admit(t0 + Duration::from_millis(1)));
        gate.set_active(true);
        assert!(gate.admit(t0 + Duration::from_millis(1)));
        assert!(gate.admit(t0 + Duration::from_millis(2)));
        gate.set_active(false);
        assert!(gate.admit(t0 + Duration::from_millis(3)), "first frame after deactivation passes");
    }

    #[test]
    fn copy_rows_handles_stride_mismatch() {
        let src: Vec<u8> = (0..24).collect(); // 2 rows, stride 12
        let mut dst = vec![0u8; 16]; // stride 8
        copy_rows(&mut dst, 8, &src, 12, 8, 2);
        assert_eq!(&dst[..8], &src[..8]);
        assert_eq!(&dst[8..16], &src[12..20]);
    }

    #[test]
    fn copy_rows_contiguous_fast_path() {
        let src: Vec<u8> = (0..16).collect();
        let mut dst = vec![0u8; 16];
        copy_rows(&mut dst, 8, &src, 8, 8, 2);
        assert_eq!(src, dst);
    }
}
