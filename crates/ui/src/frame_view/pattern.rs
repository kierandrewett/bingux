//! [`PatternSource`]: a producer that draws a known animated picture. It is
//! the producer the taxonomy shows and the tests drive, and it is a complete
//! [`FrameSource`]: a pool of buffers, a mailbox that keeps only the newest
//! frame, a waker, and a thread that renders at a steady rate (or no thread,
//! for a test that steps it by hand). It is not a mock: it is the smallest real
//! producer, and the reference for writing one.
//!
//! The picture is made so that a mistake shows. A one pixel white border and
//! an 8 px marker in each corner (red top left, green top right, blue bottom
//! left, yellow bottom right) show a crop, a shift or a flip; a checkerboard of
//! 16 px squares with a 1 px offset per frame turns any blur into grey; and
//! the frame number is drawn as 16 black and white cells along the top, so a
//! test reads which frame it is looking at from the pixels themselves
//! ([`PatternSource::frame_number`]).

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::task::Waker;
use std::thread;
use std::time::Duration;

use super::frame::{Frame, PixelBytes, PixelFormat, PixelFrame};
use super::input::{CaretRect, CursorKind};
use super::source::FrameSource;

/// Buffers in the pool: one on screen, one waiting to be taken and one being
/// drawn into.
const SLOTS: usize = 3;
/// The checkerboard's square, in pixels.
const CELL: u32 = 16;
/// The counter: this many cells, each this wide and tall, from this corner.
const BITS: u32 = 16;
const BIT: u32 = 6;
const BIT_ORIGIN: u32 = 12;
/// The smallest frame that carries the counter.
pub const PATTERN_MIN_WIDTH: u32 = BIT_ORIGIN + BITS * BIT + BIT_ORIGIN;
/// The corner markers' side.
const MARK: u32 = 8;
/// The hue band along the bottom: its height, and the square's side.
const BAND: u32 = 24;
const SQUARE: u32 = 48;

const BORDER: [u8; 4] = [255, 255, 255, 255];
const RED: [u8; 4] = [40, 40, 230, 255];
const GREEN: [u8; 4] = [60, 200, 60, 255];
const BLUE: [u8; 4] = [230, 90, 40, 255];
const YELLOW: [u8; 4] = [40, 220, 230, 255];
const ORANGE: [u8; 4] = [30, 140, 250, 255];
const ONE: [u8; 4] = [255, 255, 255, 255];
const ZERO: [u8; 4] = [0, 0, 0, 255];
const DARK: [u8; 4] = [38, 36, 36, 255];
const LIGHT: [u8; 4] = [58, 56, 56, 255];

/// What a producer has done, for a test to check: every frame it drew, how
/// many the view took and gave back, and how many it dropped because a newer
/// one was ready first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PatternStats {
    /// Frames drawn.
    pub rendered: u64,
    /// Frames handed out by `take_frame`.
    pub taken: u64,
    /// Frames handed back by `release`.
    pub released: u64,
    /// Frames replaced by a newer one before they were taken, plus frames
    /// that were drawn too late for a free buffer and skipped.
    pub dropped: u64,
    /// Releases of a frame that was not out: a view that gives one back twice.
    pub stray_releases: u64,
}

struct Buf(Vec<u8>);

impl AsRef<[u8]> for Buf {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Use {
    Free,
    /// Taken out of the pool while it is drawn into.
    Drawing,
    /// Drawn, and the newest: the next `take_frame` hands it out.
    Waiting,
    /// With the view, until `release`.
    Out,
}

struct Slot {
    /// `None` while the buffer is out of the pool being drawn into.
    buf: Option<Arc<Buf>>,
    width: u32,
    height: u32,
    format: PixelFormat,
    scale: f32,
    y_inverted: bool,
    state: Use,
}

struct State {
    width: u32,
    height: u32,
    format: PixelFormat,
    scale: f32,
    y_inverted: bool,
    slots: [Option<Slot>; SLOTS],
    waiting: Option<usize>,
    next_frame: u64,
    cursor: CursorKind,
    caret: Option<CaretRect>,
    stats: PatternStats,
}

struct Inner {
    state: Mutex<State>,
    waker: Mutex<Option<Waker>>,
    /// Set to end the thread; the condvar wakes it early.
    stop: Mutex<bool>,
    tick: Condvar,
    period: Duration,
}

/// A deterministic animated picture as a [`FrameSource`]. Make it with
/// [`PatternSource::animated`] for a steady rate, or [`PatternSource::stepped`]
/// to draw each frame by calling [`PatternSource::step`].
pub struct PatternSource {
    inner: Arc<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PatternSource {
    /// A source that draws `fps` frames a second on a thread of its own, at
    /// `width` x `height` pixels, until the last handle to it is dropped. The
    /// thread sleeps on a condition variable between frames; nothing polls.
    pub fn animated(width: u32, height: u32, fps: u32) -> Arc<Self> {
        let source = Self::build(
            width,
            height,
            Duration::from_secs_f64(1.0 / f64::from(fps.max(1))),
        );
        let inner = source.inner.clone();
        // A thread that cannot start leaves a source that only `step` drives.
        let _ = thread::Builder::new()
            .name("frame-pattern".into())
            .spawn(move || {
                loop {
                    {
                        let stopped = lock(&inner.stop);
                        let (stopped, _) = inner
                            .tick
                            .wait_timeout_while(stopped, inner.period, |stop| !*stop)
                            .unwrap_or_else(PoisonError::into_inner);
                        if *stopped {
                            return;
                        }
                    }
                    inner.render_next();
                }
            });
        source
    }

    /// A source that draws a frame only when [`step`](PatternSource::step) is
    /// called: for a test, where the frame on screen must be known.
    pub fn stepped(width: u32, height: u32) -> Arc<Self> {
        Self::build(width, height, Duration::ZERO)
    }

    fn build(width: u32, height: u32, period: Duration) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State {
                    width: width.max(1),
                    height: height.max(1),
                    format: PixelFormat::Bgrx8,
                    scale: 1.0,
                    y_inverted: false,
                    slots: [None, None, None],
                    waiting: None,
                    next_frame: 0,
                    cursor: CursorKind::Default,
                    caret: None,
                    stats: PatternStats::default(),
                }),
                waker: Mutex::new(None),
                stop: Mutex::new(false),
                tick: Condvar::new(),
                period,
            }),
        })
    }

    /// Draws the next frame now and wakes the view.
    pub fn step(&self) {
        self.inner.render_next();
    }

    /// Draws at `width` x `height` device pixels from now on, for a view of
    /// `scale` device pixels per logical pixel: what a producer does when
    /// the view reports its size. Frames already drawn at the old size are
    /// dropped or retired when the view hands them back.
    pub fn resize(&self, width: u32, height: u32, scale: f32) {
        let mut state = lock(&self.inner.state);
        state.width = width.max(1);
        state.height = height.max(1);
        state.scale = scale;
        if let Some(waiting) = state.waiting.take() {
            state.slots[waiting] = None;
            state.stats.dropped += 1;
        }
        for slot in &mut state.slots {
            if matches!(slot, Some(found) if found.state == Use::Free) {
                *slot = None;
            }
        }
    }

    /// Draws opaque (`Bgrx8`, the default) or with alpha.
    pub fn set_format(&self, format: PixelFormat) {
        lock(&self.inner.state).format = format;
    }

    /// Draws the picture upside down, as a frame read back from OpenGL is, and
    /// marks the frame `y_inverted` so it is shown upright.
    pub fn set_y_inverted(&self, y_inverted: bool) {
        lock(&self.inner.state).y_inverted = y_inverted;
    }

    /// Asks for a pointer shape over the view and wakes it.
    pub fn set_cursor(&self, cursor: CursorKind) {
        lock(&self.inner.state).cursor = cursor;
        self.inner.wake();
    }

    /// Reports a text caret and wakes the view.
    pub fn set_caret(&self, caret: Option<CaretRect>) {
        lock(&self.inner.state).caret = caret;
        self.inner.wake();
    }

    /// What has happened so far.
    pub fn stats(&self) -> PatternStats {
        lock(&self.inner.state).stats
    }

    /// The frame number drawn into `frame`'s counter cells, or `None` when the
    /// frame is too small to carry them or is not a pattern.
    pub fn frame_number(frame: &PixelFrame) -> Option<u64> {
        if frame.width < PATTERN_MIN_WIDTH || frame.height < BIT_ORIGIN + BIT {
            return None;
        }
        let row = if frame.y_inverted {
            frame.height - 1 - (BIT_ORIGIN + BIT / 2)
        } else {
            BIT_ORIGIN + BIT / 2
        };
        let mut number = 0u64;
        for bit in 0..BITS {
            let x = BIT_ORIGIN + bit * BIT + BIT / 2;
            let at = row as usize * frame.stride + x as usize * 4;
            let pixel = frame.pixels().get(at..at + 3)?;
            let on = pixel.iter().all(|&channel| channel > 200);
            let off = pixel.iter().all(|&channel| channel < 60);
            if !on && !off {
                return None;
            }
            number = (number << 1) | u64::from(on);
        }
        Some(number)
    }
}

impl Drop for PatternSource {
    fn drop(&mut self) {
        // The thread notices at once and ends; nothing waits for it.
        *lock(&self.inner.stop) = true;
        self.inner.tick.notify_all();
    }
}

impl Inner {
    fn wake(&self) {
        if let Some(waker) = lock(&self.waker).as_ref() {
            waker.wake_by_ref();
        }
    }

    /// Draws one frame into a free buffer and makes it the waiting one.
    fn render_next(&self) {
        // Take a buffer out of the pool under the lock, draw into it without.
        let (index, mut buf, width, height, format, scale, y_inverted, number) = {
            let mut state = lock(&self.state);
            let (width, height, format, scale, y_inverted) = (
                state.width,
                state.height,
                state.format,
                state.scale,
                state.y_inverted,
            );
            let Some(index) = state.acquire() else {
                state.stats.dropped += 1;
                return;
            };
            let number = state.next_frame;
            state.next_frame += 1;
            let slot = state.slots[index]
                .as_mut()
                .expect("acquire returns a used slot");
            let buf = slot.buf.take().expect("a free slot has its buffer");
            slot.state = Use::Drawing;
            (index, buf, width, height, format, scale, y_inverted, number)
        };
        match Arc::get_mut(&mut buf) {
            Some(bytes) => {
                bytes.0.resize(width as usize * height as usize * 4, 0);
                draw(&mut bytes.0, width, height, number, y_inverted);
            }
            None => {
                // Someone still holds the bytes: not ours to write.
                let mut state = lock(&self.state);
                state.slots[index] = None;
                state.stats.dropped += 1;
                return;
            }
        }
        {
            let mut state = lock(&self.state);
            if let Some(slot) = state.slots[index].as_mut() {
                slot.buf = Some(buf);
                slot.width = width;
                slot.height = height;
                slot.format = format;
                slot.scale = scale;
                slot.y_inverted = y_inverted;
                slot.state = Use::Waiting;
            }
            if let Some(older) = state.waiting.replace(index) {
                // A newer frame is ready before the older was taken: the
                // older is never shown.
                if let Some(slot) = state.slots[older].as_mut() {
                    slot.state = Use::Free;
                }
                state.stats.dropped += 1;
            }
            state.stats.rendered += 1;
        }
        self.wake();
    }
}

impl State {
    /// A free slot of the current size, making one if the pool has room.
    fn acquire(&mut self) -> Option<usize> {
        let size = (self.width, self.height);
        for slot in &mut self.slots {
            // A free buffer of another size is of no use.
            if matches!(slot, Some(found) if found.state == Use::Free && (found.width, found.height) != size)
            {
                *slot = None;
            }
        }
        if let Some(index) = self.slots.iter().position(
            |slot| matches!(slot, Some(found) if found.state == Use::Free && found.buf.is_some()),
        ) {
            return Some(index);
        }
        let index = self.slots.iter().position(Option::is_none)?;
        self.slots[index] = Some(Slot {
            buf: Some(Arc::new(Buf(Vec::new()))),
            width: size.0,
            height: size.1,
            format: self.format,
            scale: self.scale,
            y_inverted: self.y_inverted,
            state: Use::Free,
        });
        Some(index)
    }
}

impl FrameSource for PatternSource {
    fn size_hint(&self) -> Option<(f32, f32)> {
        let state = lock(&self.inner.state);
        Some((
            state.width as f32 / state.scale,
            state.height as f32 / state.scale,
        ))
    }

    fn take_frame(&self) -> Option<Frame> {
        let mut state = lock(&self.inner.state);
        let index = state.waiting.take()?;
        let slot = state.slots[index].as_mut()?;
        let bytes: PixelBytes = slot.buf.clone()?;
        let built = PixelFrame::new(
            slot.width,
            slot.height,
            slot.width as usize * 4,
            slot.format,
            bytes,
        )
        .and_then(|frame| frame.with_scale(slot.scale));
        let Ok(frame) = built else {
            // Not a frame a view can show: the buffer goes back to the pool.
            slot.state = Use::Free;
            return None;
        };
        slot.state = Use::Out;
        let frame = frame.with_y_inverted(slot.y_inverted).with_id(index as u64);
        state.stats.taken += 1;
        Some(frame.into())
    }

    fn set_waker(&self, waker: Waker) {
        *lock(&self.inner.waker) = Some(waker);
    }

    fn release(&self, frame: Frame) {
        let Frame::Pixels(frame) = frame;
        let index = frame.id as usize;
        // The frame's share of the bytes goes first, so the buffer is ours
        // alone again before it is marked free.
        drop(frame);
        let mut state = lock(&self.inner.state);
        state.stats.released += 1;
        let current = (state.width, state.height);
        // Whether the slot was out and still the size drawn now.
        let was_out = match state.slots.get(index) {
            Some(Some(found)) if found.state == Use::Out => {
                Some((found.width, found.height) == current)
            }
            _ => None,
        };
        match was_out {
            Some(true) => {
                if let Some(Some(found)) = state.slots.get_mut(index) {
                    found.state = Use::Free;
                }
            }
            // Drawn for a size the producer has left.
            Some(false) => state.slots[index] = None,
            None => state.stats.stray_releases += 1,
        }
    }

    fn cursor(&self) -> CursorKind {
        lock(&self.inner.state).cursor
    }

    fn caret(&self) -> Option<CaretRect> {
        lock(&self.inner.state).caret
    }
}

/// Draws frame `number` into `bytes`, `width` x `height` BGRA pixels with no
/// padding. With `y_inverted` the picture is written bottom row first.
pub(crate) fn draw(bytes: &mut [u8], width: u32, height: u32, number: u64, y_inverted: bool) {
    let mut canvas = Canvas {
        bytes,
        width,
        height,
        y_inverted,
    };
    let shift = (number % u64::from(CELL * 2)) as u32;
    // The checkerboard moves one pixel across and half a pixel down a frame.
    for y in 0..height {
        let band = ((y + shift / 2) / CELL) & 1;
        let mut x = 0;
        while x < width {
            let cell = (x + shift) / CELL;
            let end = ((cell + 1) * CELL - shift).min(width);
            let colour = if (cell + band) & 1 == 0 { DARK } else { LIGHT };
            canvas.span(y, x, end, colour);
            x = end;
        }
    }
    // A hue sweep along the bottom edge that rolls with the frame.
    if height > BAND * 2 {
        for y in height - BAND - 1..height - 1 {
            for x in 1..width.saturating_sub(1) {
                let hue = ((x as u64 * 360 / u64::from(width)) + number * 3) % 360;
                canvas.span(y, x, x + 1, hue_colour(hue as u32));
            }
        }
    }
    // A square that bounces.
    if width > SQUARE * 2 && height > SQUARE * 2 + BAND {
        let left = bounce(number * 3, width - SQUARE * 2) + SQUARE / 2;
        let top = bounce(number * 2, height - SQUARE * 2 - BAND) + SQUARE / 2;
        canvas.rect(left, top, left + SQUARE, top + SQUARE, ORANGE);
    }
    // The counter, then the border and the corners, which nothing covers.
    if width >= PATTERN_MIN_WIDTH && height >= BIT_ORIGIN + BIT {
        for bit in 0..BITS {
            let on = (number >> (BITS - 1 - bit)) & 1 == 1;
            let left = BIT_ORIGIN + bit * BIT;
            canvas.rect(
                left,
                BIT_ORIGIN,
                left + BIT,
                BIT_ORIGIN + BIT,
                if on { ONE } else { ZERO },
            );
        }
    }
    canvas.rect(0, 0, width, 1, BORDER);
    canvas.rect(0, height - 1, width, height, BORDER);
    canvas.rect(0, 0, 1, height, BORDER);
    canvas.rect(width - 1, 0, width, height, BORDER);
    let side = MARK.min(width / 2).min(height / 2);
    canvas.rect(1, 1, 1 + side, 1 + side, RED);
    canvas.rect(width - 1 - side, 1, width - 1, 1 + side, GREEN);
    canvas.rect(1, height - 1 - side, 1 + side, height - 1, BLUE);
    canvas.rect(
        width - 1 - side,
        height - 1 - side,
        width - 1,
        height - 1,
        YELLOW,
    );
}

/// A position that goes back and forth over `0..=span` as `t` grows.
fn bounce(t: u64, span: u32) -> u32 {
    if span == 0 {
        return 0;
    }
    let period = u64::from(span) * 2;
    let phase = (t % period) as u32;
    if phase <= span {
        phase
    } else {
        span * 2 - phase
    }
}

/// A fully saturated colour at `hue` degrees, as BGRA.
fn hue_colour(hue: u32) -> [u8; 4] {
    let sector = hue / 60;
    let rise = ((hue % 60) * 255 / 60) as u8;
    let fall = 255 - rise;
    let (r, g, b) = match sector {
        0 => (255, rise, 0),
        1 => (fall, 255, 0),
        2 => (0, 255, rise),
        3 => (0, fall, 255),
        4 => (rise, 0, 255),
        _ => (255, 0, fall),
    };
    [b, g, r, 255]
}

/// The pixel grid a picture is drawn on; rows are in picture space and land
/// bottom row first when the frame is inverted.
struct Canvas<'a> {
    bytes: &'a mut [u8],
    width: u32,
    height: u32,
    y_inverted: bool,
}

impl Canvas<'_> {
    /// Pixels `x0..x1` of picture row `y`.
    fn span(&mut self, y: u32, x0: u32, x1: u32, colour: [u8; 4]) {
        if y >= self.height || x0 >= x1 {
            return;
        }
        let row = if self.y_inverted {
            self.height - 1 - y
        } else {
            y
        } as usize;
        let start = (row * self.width as usize + x0 as usize) * 4;
        let end = (row * self.width as usize + x1.min(self.width) as usize) * 4;
        for pixel in self.bytes[start..end].chunks_exact_mut(4) {
            pixel.copy_from_slice(&colour);
        }
    }

    fn rect(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, colour: [u8; 4]) {
        for y in y0..y1.min(self.height) {
            self.span(y, x0, x1, colour);
        }
    }
}
