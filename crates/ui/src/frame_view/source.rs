//! The producer's side: the [`FrameSource`] trait and the wake-up the view
//! gives it.

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Wake, Waker};

use super::frame::Frame;
use super::input::{CaretRect, CursorKind};

/// Something that makes frames: a decoder, a remote desktop session, a game
/// or another process rendering into shared memory. The view pulls from it;
/// it never pushes pixels, so a producer that is busy or slow costs the UI
/// nothing.
///
/// Every method is called on the UI thread and must return at once: no
/// locks held across work, no I/O. A producer that works in other threads
/// keeps its state behind a short mutex or atomics, as [`PatternSource`] does.
///
/// [`PatternSource`]: super::PatternSource
pub trait FrameSource: Send + Sync + 'static {
    /// The size, in logical pixels, the producer would like to be shown at (a
    /// video's natural size), or `None`. Only a view built with
    /// [`FrameView::natural_size`] reads it.
    ///
    /// [`FrameView::natural_size`]: super::FrameView::natural_size
    fn size_hint(&self) -> Option<(f32, f32)> {
        None
    }

    /// The newest frame the view has not been given yet, or `None` when there
    /// is none. Never blocks. When several frames arrived since the last
    /// call, return the newest and recycle the rest yourself: the view never
    /// sees a frame it was not given, and it never needs to catch up.
    ///
    /// The view reads the frame's bytes until it calls [`release`] with it.
    /// It shows one frame at a time and holds the newest it was given until
    /// the next arrives, so a pool needs three buffers: the one on screen,
    /// the one being handed over and the one being drawn into.
    ///
    /// [`release`]: FrameSource::release
    fn take_frame(&self) -> Option<Frame>;

    /// Stores the waker the producer calls when there is something new: a
    /// frame, or a change of [`cursor`] or [`caret`]. Calling it is cheap, safe
    /// from any thread and coalesces: many calls before the UI looks make one
    /// redraw. Called once when the view mounts, and again if it is mounted
    /// again, which replaces the old waker. There is no polling timer: a
    /// producer that never wakes the view is never redrawn.
    ///
    /// [`cursor`]: FrameSource::cursor
    /// [`caret`]: FrameSource::caret
    fn set_waker(&self, waker: Waker);

    /// Takes a frame back after the view has drawn it and no longer reads
    /// it. Every frame [`take_frame`] returned comes back here exactly once:
    /// when a newer one replaces it, when the view unmounts, or at once when
    /// the frame changed nothing.
    ///
    /// [`take_frame`]: FrameSource::take_frame
    fn release(&self, frame: Frame);

    /// The pointer shape to show over the view. Read after every wake.
    fn cursor(&self) -> CursorKind {
        CursorKind::Default
    }

    /// Where the text caret is, in the view's logical pixels, while the
    /// producer's focus is in something that takes text; `None` otherwise.
    /// `Some` turns the input method on for the window and puts its popup by
    /// the caret; the view's accessible role becomes a text input meanwhile.
    /// Read after every wake.
    fn caret(&self) -> Option<CaretRect> {
        None
    }
}

/// The waker the view gives a producer, and the future the view's task waits
/// on. A wake sets a flag and wakes the task, whichever thread it is called
/// from; the flag makes a wake that lands before the task looks count.
pub(crate) struct WakeSignal {
    pending: AtomicBool,
    task: Mutex<Option<Waker>>,
}

impl WakeSignal {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            pending: AtomicBool::new(false),
            task: Mutex::new(None),
        })
    }

    /// The waker for the producer.
    pub(crate) fn waker(self: &Arc<Self>) -> Waker {
        Waker::from(self.clone())
    }

    /// Marks a wake without a producer: the view's first look for a frame the
    /// producer made before it was mounted.
    pub(crate) fn mark(&self) {
        self.pending.store(true, Ordering::Release);
    }

    /// Resolves once there has been a wake since the last time it resolved.
    pub(crate) fn notified(self: &Arc<Self>) -> Notified {
        Notified {
            signal: self.clone(),
        }
    }
}

impl Wake for WakeSignal {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.pending.store(true, Ordering::Release);
        if let Some(task) = self
            .task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            task.wake_by_ref();
        }
    }
}

pub(crate) struct Notified {
    signal: Arc<WakeSignal>,
}

impl Future for Notified {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let signal = &self.signal;
        if signal.pending.swap(false, Ordering::AcqRel) {
            return Poll::Ready(());
        }
        {
            let mut task = signal.task.lock().unwrap_or_else(PoisonError::into_inner);
            match task.as_mut() {
                Some(stored) => stored.clone_from(cx.waker()),
                None => *task = Some(cx.waker().clone()),
            }
        }
        // A wake between the check above and the store would have found no
        // task to wake, so look again.
        if signal.pending.swap(false, Ordering::AcqRel) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
