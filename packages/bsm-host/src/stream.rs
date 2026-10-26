//! PipeWire consumer thread: receives Mutter's virtual-monitor streams,
//! copies each frame into a free shared-memory slot and hands it to the main
//! thread.
//!
//! Format negotiation is deliberate: only BGRx and BGRA are offered (both are
//! bytes B, G, R, X/A in memory, which is `wl_shm` XRGB8888; the alpha channel
//! of BGRA is ignored because the surface is declared opaque). The size is
//! fixed to the bsm configure size and the frame rate is capped at the bsm
//! output's refresh rate, which Mutter uses as the virtual monitor mode. Buffer
//! types are limited to MemFd and MemPtr (shared memory); DMA-BUF is not
//! offered, so Mutter never negotiates it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use pipewire as pw;
use pw::spa;
use pw::spa::buffer::meta::{MetaCursor, MetaVideoDamage};
use pw::spa::param::video::{VideoFormat, VideoInfoRaw};
use pw::spa::pod::{Object, Pod, Property, PropertyFlags, Value, serialize::PodSerializer};
use pw::spa::sys;
use pw::spa::utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle, SpaTypes};

use crate::frames::{BPP, OutputGate, copy_rows};
use crate::msg::{CursorImage, Msg, Rect, StreamStatus};

pub enum Cmd {
    Add {
        output: String,
        epoch: u64,
        node_id: u32,
        width: u32,
        height: u32,
        refresh_mhz: u32,
        gate: Arc<OutputGate>,
    },
    Remove { output: String },
    Quit,
}

pub struct StreamThread {
    pub tx: pw::channel::Sender<Cmd>,
    pub join: JoinHandle<()>,
}

struct StreamData {
    output: String,
    epoch: u64,
    gate: Arc<OutputGate>,
    tx: calloop::channel::Sender<Msg>,
    format: VideoInfoRaw,
    want: (u32, u32),
    seq: u64,
    cursor_id: u32,
    streaming_reported: bool,
    /// The newest frame withheld by background throttling. Forwarded when the
    /// interval elapses so the last state is never lost to throttling.
    pending: Option<pw::buffer::Buffer<'static>>,
    /// Damage of every frame received since the last one that was forwarded.
    /// Frames dropped by throttling or for lack of a free slot still changed
    /// the picture, so their damage is carried into the next forwarded frame.
    damage: Damage,
}

/// Accumulated damage in buffer pixels. `Full` means the whole frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Damage {
    Rects(Vec<Rect>),
    Full,
}

const MAX_DAMAGE_RECTS: usize = 16;

impl Damage {
    pub fn none() -> Self {
        Damage::Rects(Vec::new())
    }

    /// Add one frame's damage; an empty list means the frame carried no
    /// damage information and is treated as a full update.
    pub fn add(&mut self, frame: &[Rect]) {
        let Damage::Rects(rects) = self else { return };
        if frame.is_empty() {
            *self = Damage::Full;
            return;
        }
        rects.extend_from_slice(frame);
        if rects.len() > MAX_DAMAGE_RECTS {
            // Collapse to the bounding box instead of growing without bound.
            let (x0, y0, x1, y1) = rects.iter().fold((i32::MAX, i32::MAX, i32::MIN, i32::MIN), |m, r| {
                (m.0.min(r.x), m.1.min(r.y), m.2.max(r.x + r.w), m.3.max(r.y + r.h))
            });
            *rects = vec![Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }];
        }
    }

    /// The rectangles to send; empty means "everything" to the Wayland side.
    pub fn take(&mut self) -> Vec<Rect> {
        match std::mem::replace(self, Damage::none()) {
            Damage::Full => Vec::new(),
            Damage::Rects(r) => r,
        }
    }
}

type Shared = Rc<RefCell<StreamData>>;

struct Entry {
    // Field order matters: the pending buffer must be dropped before the stream.
    data: Shared,
    _listener: pw::stream::StreamListener<Shared>,
    stream: pw::stream::StreamRc,
}

impl Entry {
    fn close(self) {
        self.data.borrow_mut().pending = None;
        let _ = self.stream.disconnect();
    }
}

pub fn spawn(tx: calloop::channel::Sender<Msg>) -> Result<StreamThread> {
    let (cmd_tx, cmd_rx) = pw::channel::channel::<Cmd>();
    let join = std::thread::Builder::new()
        .name("bsm-host-pw".into())
        .spawn(move || {
            if let Err(e) = run(cmd_rx, tx.clone()) {
                log::error!("pipewire thread: {e:#}");
                let _ = tx.send(Msg::GnoblinLost(format!("pipewire: {e:#}")));
            }
        })
        .context("spawn pipewire thread")?;
    Ok(StreamThread { tx: cmd_tx, join })
}

fn run(cmd_rx: pw::channel::Receiver<Cmd>, tx: calloop::channel::Sender<Msg>) -> Result<()> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).context("pipewire main loop")?;
    let context = pw::context::ContextRc::new(&mainloop, None).context("pipewire context")?;
    let core = context
        .connect_rc(None)
        .context("connect to the session PipeWire instance")?;
    let entries: Rc<RefCell<HashMap<String, Entry>>> = Rc::default();

    let quit_loop = mainloop.clone();
    let _attached = {
        let entries = entries.clone();
        cmd_rx.attach(mainloop.loop_(), move |cmd| match cmd {
            Cmd::Quit => quit_loop.quit(),
            Cmd::Remove { output } => {
                if let Some(e) = entries.borrow_mut().remove(&output) {
                    e.close();
                }
            }
            Cmd::Add { output, epoch, node_id, width, height, refresh_mhz, gate } => {
                match add_stream(&core, &tx, &output, epoch, node_id, width, height, refresh_mhz, gate) {
                    Ok(entry) => {
                        if let Some(old) = entries.borrow_mut().insert(output.clone(), entry) {
                            old.close();
                        }
                    }
                    Err(e) => {
                        log::error!("stream for {output}: {e:#}");
                        let _ = tx.send(Msg::Stream {
                            output,
                            epoch,
                            status: StreamStatus::Failed(format!("{e:#}")),
                        });
                    }
                }
            }
        })
    };
    // Flush frames withheld by background throttling once they are due.
    let timer = mainloop.loop_().add_timer({
        let entries = entries.clone();
        move |_| {
            for e in entries.borrow().values() {
                let mut d = e.data.borrow_mut();
                if d.pending.is_some() && (d.gate.is_active() || d.gate.admit(Instant::now())) {
                    if let Some(mut b) = d.pending.take() {
                        forward(&mut b, &mut d);
                    }
                }
            }
        }
    });
    timer
        .update_timer(Some(Duration::from_millis(20)), Some(Duration::from_millis(20)))
        .into_result()
        .context("arm throttle timer")?;
    mainloop.run();
    let drained: Vec<Entry> = entries.borrow_mut().drain().map(|(_, e)| e).collect();
    for e in drained {
        e.close();
    }
    Ok(())
}

fn serialize(obj: Object) -> Vec<u8> {
    PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(obj))
        .expect("serialise pod")
        .0
        .into_inner()
}

fn prop(key: u32, value: Value) -> Property {
    Property { key, flags: PropertyFlags::empty(), value }
}

fn enum_format(width: u32, height: u32, refresh_mhz: u32) -> Vec<u8> {
    let fps = Fraction { num: refresh_mhz.max(1000), denom: 1000 };
    serialize(Object {
        type_: SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: vec![
            prop(sys::SPA_FORMAT_mediaType, Value::Id(Id(sys::SPA_MEDIA_TYPE_video))),
            prop(sys::SPA_FORMAT_mediaSubtype, Value::Id(Id(sys::SPA_MEDIA_SUBTYPE_raw))),
            prop(
                sys::SPA_FORMAT_VIDEO_format,
                Value::Choice(spa::pod::ChoiceValue::Id(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Enum {
                        default: Id(VideoFormat::BGRx.as_raw()),
                        alternatives: vec![
                            Id(VideoFormat::BGRx.as_raw()),
                            Id(VideoFormat::BGRA.as_raw()),
                        ],
                    },
                ))),
            ),
            prop(sys::SPA_FORMAT_VIDEO_size, Value::Rectangle(Rectangle { width, height })),
            prop(sys::SPA_FORMAT_VIDEO_framerate, Value::Fraction(Fraction { num: 0, denom: 1 })),
            prop(
                sys::SPA_FORMAT_VIDEO_maxFramerate,
                Value::Choice(spa::pod::ChoiceValue::Fraction(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Range {
                        default: fps,
                        min: Fraction { num: 1, denom: 1 },
                        max: fps,
                    },
                ))),
            ),
        ],
    })
}

fn buffers_param(width: u32, height: u32) -> Vec<u8> {
    let stride = width as i32 * BPP as i32;
    let size = stride * height as i32;
    serialize(Object {
        type_: SpaTypes::ObjectParamBuffers.as_raw(),
        id: spa::param::ParamType::Buffers.as_raw(),
        properties: vec![
            prop(
                sys::SPA_PARAM_BUFFERS_buffers,
                Value::Choice(spa::pod::ChoiceValue::Int(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Range { default: 4, min: 2, max: 8 },
                ))),
            ),
            prop(sys::SPA_PARAM_BUFFERS_blocks, Value::Int(1)),
            prop(sys::SPA_PARAM_BUFFERS_size, Value::Int(size)),
            prop(sys::SPA_PARAM_BUFFERS_stride, Value::Int(stride)),
            prop(
                sys::SPA_PARAM_BUFFERS_dataType,
                Value::Choice(spa::pod::ChoiceValue::Int(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Flags {
                        default: (1 << sys::SPA_DATA_MemFd) | (1 << sys::SPA_DATA_MemPtr),
                        flags: vec![(1 << sys::SPA_DATA_MemFd) | (1 << sys::SPA_DATA_MemPtr)],
                    },
                ))),
            ),
        ],
    })
}

fn meta_param(kind: u32, size: i32) -> Vec<u8> {
    serialize(Object {
        type_: SpaTypes::ObjectParamMeta.as_raw(),
        id: spa::param::ParamType::Meta.as_raw(),
        properties: vec![
            prop(sys::SPA_PARAM_META_type, Value::Id(Id(kind))),
            prop(sys::SPA_PARAM_META_size, Value::Int(size)),
        ],
    })
}

#[allow(clippy::too_many_arguments)]
fn add_stream(
    core: &pw::core::CoreRc,
    tx: &calloop::channel::Sender<Msg>,
    output: &str,
    epoch: u64,
    node_id: u32,
    width: u32,
    height: u32,
    refresh_mhz: u32,
    gate: Arc<OutputGate>,
) -> Result<Entry> {
    let stream = pw::stream::StreamRc::new(
        core.clone(),
        &format!("bsm-host {output}"),
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .context("create stream")?;

    let data = Rc::new(RefCell::new(StreamData {
        output: output.to_owned(),
        epoch,
        gate,
        tx: tx.clone(),
        format: VideoInfoRaw::default(),
        want: (width, height),
        seq: 0,
        cursor_id: 0,
        streaming_reported: false,
        pending: None,
        damage: Damage::none(),
    }));

    let listener = stream
        .add_local_listener_with_user_data(data.clone())
        .state_changed(|_, d, old, new| {
            let d = d.borrow();
            log::debug!("stream {}: {old:?} -> {new:?}", d.output);
            use pw::stream::StreamState as S;
            match new {
                S::Error(e) => {
                    let _ = d.tx.send(Msg::Stream {
                        output: d.output.clone(),
                        epoch: d.epoch,
                        status: StreamStatus::Failed(e),
                    });
                }
                S::Unconnected if old != S::Unconnected => {
                    let _ = d.tx.send(Msg::Stream {
                        output: d.output.clone(),
                        epoch: d.epoch,
                        status: StreamStatus::Ended,
                    });
                }
                _ => {}
            }
        })
        .param_changed(|stream, d, id, param| {
            let mut d = d.borrow_mut();
            let d = &mut *d;
            log::debug!("stream {}: param_changed id {id} set={}", d.output, param.is_some());
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Some(param) = param else {
                return;
            };
            if let Err(e) = d.format.parse(param) {
                log::error!("stream {}: cannot parse the negotiated format: {e:?}", d.output);
                return;
            }
            let size = d.format.size();
            log::info!(
                "stream {}: negotiated {:?} {}x{} max {}/{}",
                d.output,
                d.format.format(),
                size.width,
                size.height,
                d.format.max_framerate().num,
                d.format.max_framerate().denom,
            );
            let buffers = buffers_param(size.width, size.height);
            // 16 damage rectangles; cursors up to 384x384 ARGB.
            let damage = meta_param(sys::SPA_META_VideoDamage, 16 * std::mem::size_of::<sys::spa_meta_region>() as i32);
            let cursor_size = (std::mem::size_of::<sys::spa_meta_cursor>()
                + std::mem::size_of::<sys::spa_meta_bitmap>()
                + 384 * 384 * 4) as i32;
            let cursor = meta_param(sys::SPA_META_Cursor, cursor_size);
            let header = meta_param(sys::SPA_META_Header, std::mem::size_of::<sys::spa_meta_header>() as i32);
            let pods: Vec<&Pod> = [&buffers, &damage, &cursor, &header]
                .iter()
                .filter_map(|b| Pod::from_bytes(b))
                .collect();
            let mut pods = pods;
            if let Err(e) = stream.update_params(&mut pods) {
                log::error!("stream {}: update_params: {e}", d.output);
            }
            if !d.streaming_reported {
                d.streaming_reported = true;
                let _ = d.tx.send(Msg::Stream {
                    output: d.output.clone(),
                    epoch: d.epoch,
                    status: StreamStatus::Streaming { width: size.width, height: size.height },
                });
            }
        })
        .process(|stream, d| process(stream, &mut d.borrow_mut()))
        .register()
        .context("register stream listener")?;

    let fmt = enum_format(width, height, refresh_mhz);
    let mut params = [Pod::from_bytes(&fmt).context("format pod")?];
    stream
        .connect(
            spa::utils::Direction::Input,
            Some(node_id),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .context("connect stream")?;
    Ok(Entry { data, _listener: listener, stream })
}

fn process(stream: &pw::stream::Stream, d: &mut StreamData) {
    let Some(mut buffer) = stream.dequeue_buffer() else { return };
    d.gate.stats.frames_in.fetch_add(1, Ordering::Relaxed);
    handle_cursor(&buffer, d);
    let frame_damage = buffer
        .find_meta::<MetaVideoDamage>()
        .map(|m| {
            m.iter()
                .map(|r| {
                    let p = r.position();
                    let s = r.size();
                    Rect { x: p.x, y: p.y, w: s.width as i32, h: s.height as i32 }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    d.damage.add(&frame_damage);
    if d.gate.is_active() || d.gate.admit(Instant::now()) {
        d.pending = None;
        forward(&mut buffer, d);
    } else {
        // Background: withhold the newest frame; the timer forwards it when due.
        d.gate.stats.dropped_throttle.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the buffer borrows the stream, which lives in the same `Entry`
        // and outlives `pending` (see `Entry::close` and field order).
        d.pending = Some(unsafe { std::mem::transmute::<pw::buffer::Buffer<'_>, pw::buffer::Buffer<'static>>(buffer) });
    }
}

/// Copy the frame in `buffer` into a free slot and send it to the main thread.
fn forward(buffer: &mut pw::buffer::Buffer<'_>, d: &mut StreamData) {
    let stats = &d.gate.stats;
    let size = d.format.size();
    if (size.width, size.height) != d.want || size.width == 0 {
        stats.dropped_size.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let Some(set) = d.gate.buffers() else {
        stats.dropped_no_set.fetch_add(1, Ordering::Relaxed);
        return;
    };
    if set.width != size.width || set.height != size.height {
        stats.dropped_size.fetch_add(1, Ordering::Relaxed);
        return;
    }

    let Some(data) = buffer.datas_mut().first_mut() else { return };
    let chunk_size = data.chunk().size() as usize;
    let chunk_offset = data.chunk().offset() as usize;
    let src_stride = match data.chunk().stride() {
        s if s > 0 => s as usize,
        _ => size.width as usize * BPP,
    };
    let Some(src) = data.data() else { return };
    let row_bytes = size.width as usize * BPP;
    let need = src_stride * (size.height as usize - 1) + row_bytes;
    if chunk_size == 0 || chunk_offset + need > src.len() {
        // No new image (for example a cursor-only update) or a short buffer.
        return;
    }
    let Some(slot) = set.claim() else {
        stats.dropped_no_slot.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let t0 = Instant::now();
    // SAFETY: `slot` was claimed above and is released only by the main thread
    // after it has committed (or dropped) the frame message.
    let dst = unsafe { set.slot_mut(slot) };
    copy_rows(dst, set.stride as usize, &src[chunk_offset..], src_stride, row_bytes, size.height as usize);
    stats.copy_ns.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
    stats.frames_copied.fetch_add(1, Ordering::Relaxed);
    stats.bytes_copied.fetch_add((row_bytes * size.height as usize) as u64, Ordering::Relaxed);
    d.seq += 1;
    let damage = d.damage.take();
    if d
        .tx
        .send(Msg::Frame {
            output: d.output.clone(),
            epoch: d.epoch,
            set: set.clone(),
            slot,
            damage,
            seq: d.seq,
        })
        .is_err()
    {
        set.release(slot);
    }
}

fn handle_cursor(buffer: &pw::buffer::Buffer<'_>, d: &mut StreamData) {
    let Some(cursor) = buffer.find_meta::<MetaCursor>() else { return };
    if cursor.id() == 0 {
        return;
    }
    let Some(bitmap) = cursor.bitmap() else {
        if d.cursor_id != u32::MAX {
            d.cursor_id = u32::MAX;
            let _ = d.tx.send(Msg::CursorHidden { output: d.output.clone(), epoch: d.epoch });
        }
        return;
    };
    if cursor.id() == d.cursor_id {
        return;
    }
    let sz = bitmap.size();
    let stride = bitmap.stride();
    let Some(bytes) = bitmap.bitmap_data() else { return };
    if sz.width == 0 || sz.height == 0 || stride <= 0 {
        if d.cursor_id != u32::MAX {
            d.cursor_id = u32::MAX;
            let _ = d.tx.send(Msg::CursorHidden { output: d.output.clone(), epoch: d.epoch });
        }
        return;
    }
    let row = sz.width as usize * 4;
    let mut pixels = vec![0u8; row * sz.height as usize];
    for y in 0..sz.height as usize {
        let s = y * stride as usize;
        if s + row > bytes.len() {
            return;
        }
        pixels[y * row..(y + 1) * row].copy_from_slice(&bytes[s..s + row]);
    }
    d.cursor_id = cursor.id();
    let hot = cursor.hotspot();
    let _ = d.tx.send(Msg::Cursor {
        output: d.output.clone(),
        epoch: d.epoch,
        image: CursorImage {
            id: cursor.id(),
            width: sz.width,
            height: sz.height,
            hotspot_x: hot.x,
            hotspot_y: hot.y,
            pixels,
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn damage_of_dropped_frames_is_carried_into_the_next_one() {
        let mut d = Damage::none();
        d.add(&[r(0, 0, 10, 10)]); // dropped by throttling
        d.add(&[r(50, 50, 5, 5)]); // forwarded now
        assert_eq!(d.take(), vec![r(0, 0, 10, 10), r(50, 50, 5, 5)]);
        assert_eq!(d.take(), vec![], "damage is consumed once");
    }

    #[test]
    fn a_frame_without_damage_information_means_full_damage() {
        let mut d = Damage::none();
        d.add(&[r(0, 0, 10, 10)]);
        d.add(&[]);
        d.add(&[r(1, 1, 1, 1)]);
        assert_eq!(d.take(), vec![], "empty means the whole frame");
    }

    #[test]
    fn the_rectangle_list_stays_bounded_and_still_covers_everything() {
        let mut d = Damage::none();
        let all: Vec<Rect> = (0..40).map(|i| r(i * 10, i * 2, 5, 5)).collect();
        for x in &all {
            d.add(&[*x]);
        }
        let out = d.take();
        assert!(!out.is_empty() && out.len() <= MAX_DAMAGE_RECTS, "{} rects", out.len());
        for x in &all {
            assert!(
                out.iter().any(|o| o.x <= x.x && o.y <= x.y && o.x + o.w >= x.x + x.w && o.y + o.h >= x.y + x.h),
                "{x:?} lost"
            );
        }
    }
}
