//! The page's pixels, as the frame view wants them.
//!
//! A [`PageSurface`] is the producer side of one tab: the backend pushes each
//! frame the engine finished (with the pointer shape and the text caret), and
//! the frame view pulls the newest. It is a [`FrameSource`] with a mailbox of
//! one: a frame the view did not take before the next arrived is handed back
//! to the engine at once, with its damage folded into the newer one, so a slow
//! UI never holds a buffer the engine needs.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::Waker;

use crate::frame_view::{
    CaretRect, CursorKind, DamageRect, Frame, FrameSource, PixelBytes, PixelFormat, PixelFrame,
};

struct Held {
    frame: PixelFrame,
}

#[derive(Default)]
struct State {
    /// The newest frame, not yet taken by the view.
    waiting: Option<Held>,
    /// What changed in frames that were dropped before the view took one.
    folded: Option<DamageRect>,
    cursor: CursorKind,
    caret: Option<CaretRect>,
    /// How many frames were pushed and how many reached the view, for tests.
    pushed: u64,
    taken: u64,
    dropped: u64,
}

/// One tab's frames, pointer shape and caret.
#[derive(Default)]
pub struct PageSurface {
    state: Mutex<State>,
    waker: Mutex<Option<Waker>>,
}

/// What a surface has done, for tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceStats {
    pub pushed: u64,
    pub taken: u64,
    /// Frames replaced before the view took them.
    pub dropped: u64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PageSurface {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn wake(&self) {
        if let Some(waker) = lock(&self.waker).as_ref() {
            waker.wake_by_ref();
        }
    }

    /// A frame the engine finished. Replaces one the view did not take, and
    /// wakes the view. Damage is relative to the frame the view shows, so
    /// the dropped frame's damage is folded in.
    pub fn push(&self, mut frame: PixelFrame) {
        {
            let mut state = lock(&self.state);
            state.pushed += 1;
            crate::frame_view::stats::arrived();
            if let Some(old) = state.waiting.take() {
                state.dropped += 1;
                crate::frame_view::stats::dropped();
                let same_size = (old.frame.width, old.frame.height) == (frame.width, frame.height);
                if same_size {
                    let carried = state.folded.unwrap_or_else(|| old.frame.damage_rect());
                    let union = carried.union(frame.damage_rect());
                    frame.damage = Some(union);
                    state.folded = Some(union);
                } else {
                    state.folded = None;
                }
                drop(old);
            } else {
                state.folded = None;
            }
            state.waiting = Some(Held { frame });
        }
        self.wake();
    }

    /// The pointer shape the page asks for.
    pub fn set_cursor(&self, cursor: CursorKind) {
        {
            let mut state = lock(&self.state);
            if state.cursor == cursor {
                return;
            }
            state.cursor = cursor;
        }
        self.wake();
    }

    /// The text caret in the view's logical pixels, or `None` when no editor
    /// has the focus.
    pub fn set_caret(&self, caret: Option<CaretRect>) {
        {
            let mut state = lock(&self.state);
            if state.caret == caret {
                return;
            }
            state.caret = caret;
        }
        self.wake();
    }

    pub fn stats(&self) -> SurfaceStats {
        let state = lock(&self.state);
        SurfaceStats {
            pushed: state.pushed,
            taken: state.taken,
            dropped: state.dropped,
        }
    }

    /// The frame of a mapped buffer, ready for [`PageSurface::push`]. `bytes`
    /// is the engine's buffer, `Arc`-shared so nothing is copied; dropping the
    /// last share releases it. The page is opaque, so the fourth byte of a
    /// pixel is ignored unless `opaque` is false.
    pub fn frame_of(
        width: u32,
        height: u32,
        stride: usize,
        scale: f32,
        opaque: bool,
        damage: Option<DamageRect>,
        bytes: PixelBytes,
    ) -> Option<PixelFrame> {
        let format = if opaque {
            PixelFormat::Bgrx8
        } else {
            PixelFormat::Bgra8Premultiplied
        };
        let mut frame = PixelFrame::new(width, height, stride, format, bytes)
            .ok()?
            .with_scale(scale)
            .ok()?;
        frame.damage = damage;
        Some(frame)
    }
}

impl FrameSource for PageSurface {
    fn take_frame(&self) -> Option<Frame> {
        let mut state = lock(&self.state);
        let held = state.waiting.take()?;
        state.taken += 1;
        state.folded = None;
        Some(held.frame.into())
    }

    fn set_waker(&self, waker: Waker) {
        *lock(&self.waker) = Some(waker);
    }

    fn release(&self, frame: Frame) {
        // Dropping the frame drops the producer's buffer share, which is what
        // returns a buffer to the engine.
        drop(frame);
    }

    fn cursor(&self) -> CursorKind {
        lock(&self.state).cursor
    }

    fn caret(&self) -> Option<CaretRect> {
        lock(&self.state).caret
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// Counts how many times the buffer is let go.
    struct Pixels(Vec<u8>, Arc<AtomicUsize>);

    impl AsRef<[u8]> for Pixels {
        fn as_ref(&self) -> &[u8] {
            &self.0
        }
    }

    impl Drop for Pixels {
        fn drop(&mut self) {
            self.1.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn frame(width: u32, height: u32, damage: DamageRect, released: &Arc<AtomicUsize>) -> PixelFrame {
        let bytes: PixelBytes = Arc::new(Pixels(vec![0; (width * height * 4) as usize], released.clone()));
        PageSurface::frame_of(width, height, width as usize * 4, 1.0, true, Some(damage), bytes)
            .expect("a frame")
    }

    #[test]
    fn the_view_gets_the_newest_frame_and_the_engine_gets_the_rest_back() {
        let released = Arc::new(AtomicUsize::new(0));
        let surface = PageSurface::new();
        surface.push(frame(10, 10, DamageRect::new(0, 0, 2, 2), &released));
        surface.push(frame(10, 10, DamageRect::new(8, 8, 2, 2), &released));
        assert_eq!(released.load(Ordering::SeqCst), 1, "the frame nobody saw is back with the engine");
        let Some(Frame::Pixels(taken)) = surface.take_frame() else {
            panic!("a frame");
        };
        assert_eq!(
            taken.damage_rect(),
            DamageRect::new(0, 0, 10, 10),
            "the damage of the dropped frame is folded into the one shown"
        );
        assert!(surface.take_frame().is_none(), "a frame is handed out once");
        surface.release(Frame::Pixels(taken));
        assert_eq!(released.load(Ordering::SeqCst), 2);
        assert_eq!(
            surface.stats(),
            SurfaceStats {
                pushed: 2,
                taken: 1,
                dropped: 1
            }
        );
    }

    #[test]
    fn a_frame_of_another_size_does_not_inherit_damage() {
        let released = Arc::new(AtomicUsize::new(0));
        let surface = PageSurface::new();
        surface.push(frame(10, 10, DamageRect::new(0, 0, 2, 2), &released));
        let bigger = frame(20, 20, DamageRect::new(5, 5, 1, 1), &released);
        surface.push(bigger);
        let Some(Frame::Pixels(taken)) = surface.take_frame() else {
            panic!("a frame");
        };
        assert_eq!(taken.damage, Some(DamageRect::new(5, 5, 1, 1)));
    }

    #[test]
    fn a_waker_is_called_for_a_frame_a_cursor_and_a_caret_but_not_for_no_change() {
        struct Count(AtomicUsize);
        impl std::task::Wake for Count {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let count = Arc::new(Count(AtomicUsize::new(0)));
        let surface = PageSurface::new();
        surface.set_waker(Waker::from(count.clone()));
        let released = Arc::new(AtomicUsize::new(0));
        surface.push(frame(4, 4, DamageRect::full(4, 4), &released));
        surface.set_cursor(CursorKind::Text);
        surface.set_cursor(CursorKind::Text);
        surface.set_caret(Some(CaretRect {
            x: 1.0,
            y: 2.0,
            width: 1.0,
            height: 12.0,
        }));
        assert_eq!(count.0.load(Ordering::SeqCst), 3);
        assert_eq!(surface.cursor(), CursorKind::Text);
        assert!(surface.caret().is_some());
    }
}
