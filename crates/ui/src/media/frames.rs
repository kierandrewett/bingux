//! The pictures of a film, handed from a decoder's thread to the window.
//!
//! A player renders frames on a thread of its own; [`VideoFrames`] is the
//! mailbox between it and a [`video_surface`](super::video_surface). The
//! decoder pushes the newest frame (a frame not drawn yet is replaced, never
//! queued), and the surface wakes, takes it and draws it. The other way, the
//! surface writes the size it shows the picture at, in device pixels, so the
//! decoder can render exactly that many pixels: no scaling in the window, and
//! subtitles drawn at the screen's resolution.
//!
//! The mailbox knows nothing about the player: mpv (`bingux-media`),
//! GStreamer or a test pattern can all feed it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bytes::Bytes;

use crate::task::Feed;

/// One picture: `width * height` pixels of four bytes, red, green, blue and
/// one byte that is ignored (the picture is opaque), rows `width * 4` bytes
/// apart.
#[derive(Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Bytes,
}

enum Slot {
    /// Nothing new since the surface last looked.
    Seen,
    Frame(VideoFrame),
    /// The picture went away (a song, or nothing playing).
    Cleared,
}

type Listener = Arc<dyn Fn() + Send + Sync>;

struct Inner {
    slot: Mutex<Slot>,
    /// A wake-up is in the feed and has not been taken: pushing again only
    /// replaces the frame.
    announced: AtomicBool,
    feed: Feed<()>,
    size: Mutex<(u32, u32)>,
    on_resize: Mutex<Option<Listener>>,
}

/// The mailbox. Clones share it; send it to the decoder's thread.
#[derive(Clone)]
pub struct VideoFrames {
    inner: Arc<Inner>,
}

impl PartialEq for VideoFrames {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Default for VideoFrames {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for VideoFrames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoFrames")
            .field("size", &self.size())
            .finish()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl VideoFrames {
    pub fn new() -> Self {
        VideoFrames {
            inner: Arc::new(Inner {
                slot: Mutex::new(Slot::Seen),
                announced: AtomicBool::new(false),
                feed: Feed::default(),
                size: Mutex::new((0, 0)),
                on_resize: Mutex::new(None),
            }),
        }
    }

    /// A new picture, from any thread. A frame the surface has not drawn yet
    /// is dropped for this one.
    pub fn push(&self, frame: VideoFrame) {
        *lock(&self.inner.slot) = Slot::Frame(frame);
        self.announce();
    }

    /// No picture any more: the surface goes black.
    pub fn clear(&self) {
        *lock(&self.inner.slot) = Slot::Cleared;
        self.announce();
    }

    fn announce(&self) {
        if !self.inner.announced.swap(true, Ordering::AcqRel) {
            self.inner.feed.push(());
        }
    }

    /// The size the surface shows the picture at, in device pixels; `(0, 0)`
    /// while no surface shows it.
    pub fn size(&self) -> (u32, u32) {
        *lock(&self.inner.size)
    }

    /// Calls `listener` (on the window's thread) when the surface changes
    /// size, so a paused picture can be rendered again at the new size.
    pub fn on_resize(&self, listener: impl Fn() + Send + Sync + 'static) {
        *lock(&self.inner.on_resize) = Some(Arc::new(listener));
    }

    pub(crate) fn set_size(&self, size: (u32, u32)) {
        let changed = {
            let mut current = lock(&self.inner.size);
            let changed = *current != size;
            *current = size;
            changed
        };
        if changed {
            let listener = lock(&self.inner.on_resize).clone();
            if let Some(listener) = listener {
                listener();
            }
        }
    }

    /// What changed since the last call: `Some(Some(frame))` a new picture,
    /// `Some(None)` no picture, `None` nothing.
    pub(crate) fn take(&self) -> Option<Option<VideoFrame>> {
        self.inner.announced.store(false, Ordering::Release);
        match std::mem::replace(&mut *lock(&self.inner.slot), Slot::Seen) {
            Slot::Seen => None,
            Slot::Frame(frame) => Some(Some(frame)),
            Slot::Cleared => Some(None),
        }
    }

    /// Waits until something was pushed or cleared.
    pub(crate) async fn changed(&self) {
        self.inner.feed.next().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_frame_wins() {
        let frames = VideoFrames::new();
        let frame = |width| VideoFrame {
            width,
            height: 1,
            pixels: Bytes::from(vec![0; width as usize * 4]),
        };
        frames.push(frame(1));
        frames.push(frame(2));
        assert_eq!(frames.take().flatten().map(|frame| frame.width), Some(2));
        assert!(frames.take().is_none());
        frames.clear();
        assert!(matches!(frames.take(), Some(None)));
    }

    #[test]
    fn a_resize_is_told_once() {
        let frames = VideoFrames::new();
        let count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let seen = count.clone();
        frames.on_resize(move || {
            seen.fetch_add(1, Ordering::Relaxed);
        });
        frames.set_size((10, 10));
        frames.set_size((10, 10));
        frames.set_size((20, 10));
        assert_eq!(count.load(Ordering::Relaxed), 2);
        assert_eq!(frames.size(), (20, 10));
    }
}
