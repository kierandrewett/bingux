//! What a producer hands the view: a frame, and the pixels inside it.
//!
//! A [`Frame`] is `#[non_exhaustive]`: today it holds pixels in shared
//! memory, and a GPU buffer (a dmabuf) will be a second variant without
//! changing the code that matches on it with a wildcard arm.

use std::sync::Arc;

/// How the bytes of a pixel are laid out. `#[non_exhaustive]`: more formats
/// can follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PixelFormat {
    /// Four bytes per pixel in memory order blue, green, red, alpha, with the
    /// colour channels already multiplied by alpha. This is the layout the
    /// window's own surface has, so it is drawn with no conversion.
    Bgra8Premultiplied,
    /// The same bytes with the fourth byte ignored: every pixel is opaque.
    /// A page or a video has no transparency, and telling the renderer so
    /// lets it copy rows instead of blending each pixel.
    Bgrx8,
}

impl PixelFormat {
    /// Bytes one pixel takes.
    pub const fn bytes_per_pixel(self) -> usize {
        4
    }
}

/// The bytes of a frame: any region of memory the producer can lend, such as
/// a mapped shared-memory file, behind an `Arc` so holding it never copies it.
/// The view reads it between [`FrameSource::take_frame`] and
/// [`FrameSource::release`]; the producer must not write to it in that time.
///
/// [`FrameSource::take_frame`]: super::FrameSource::take_frame
/// [`FrameSource::release`]: super::FrameSource::release
pub type PixelBytes = Arc<dyn AsRef<[u8]> + Send + Sync>;

/// A rectangle of a frame in pixels, top left origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DamageRect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The whole of a `width` x `height` frame.
    pub const fn full(width: u32, height: u32) -> Self {
        Self::new(0, 0, width, height)
    }

    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// The smallest rectangle that holds both. An empty one adds nothing, so
    /// a producer folds the damage of the frames it skipped with this.
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = self
            .x
            .saturating_add(self.width)
            .max(other.x.saturating_add(other.width));
        let bottom = self
            .y
            .saturating_add(self.height)
            .max(other.y.saturating_add(other.height));
        Self::new(left, top, right - left, bottom - top)
    }

    /// The part of this rectangle inside a `width` x `height` frame; empty
    /// when nothing of it is.
    pub fn clamped(self, width: u32, height: u32) -> Self {
        let left = self.x.min(width);
        let top = self.y.min(height);
        let right = self.x.saturating_add(self.width).min(width);
        let bottom = self.y.saturating_add(self.height).min(height);
        Self::new(left, top, right - left, bottom - top)
    }
}

/// Why pixels could not be made into a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// A width or a height of 0.
    Empty,
    /// A row is wider than the stride that holds it.
    StrideTooSmall,
    /// The bytes end before the last row does.
    TooShort,
    /// The producer's scale is not a positive number.
    BadScale,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "a frame needs at least one pixel",
            Self::StrideTooSmall => "the stride is smaller than one row of pixels",
            Self::TooShort => "the bytes end before the last row does",
            Self::BadScale => "the frame's scale must be greater than zero",
        })
    }
}

impl std::error::Error for FrameError {}

/// A frame of pixels in memory. Made with [`PixelFrame::new`], which checks
/// that the bytes cover it; the fields stay readable.
///
/// It is not `Clone`: [`FrameSource::release`] takes the frame back by value
/// and the view holds exactly one, so a buffer is given back exactly once.
///
/// [`FrameSource::release`]: super::FrameSource::release
pub struct PixelFrame {
    /// Pixels per row and number of rows.
    pub width: u32,
    pub height: u32,
    /// Bytes from the start of one row to the start of the next.
    pub stride: usize,
    pub format: PixelFormat,
    pub bytes: PixelBytes,
    /// What changed since the frame the producer last handed out and the
    /// view still shows (not since the last frame it rendered: a producer
    /// that skipped frames folds their damage in with [`DamageRect::union`]).
    /// `None` is the whole frame. An empty rectangle means nothing changed:
    /// the view keeps what it shows, asks for no redraw and gives the buffer
    /// straight back. Ignored when the size differs from the frame on screen.
    pub damage: Option<DamageRect>,
    /// The first row is the bottom one, as an OpenGL texture reads back.
    pub y_inverted: bool,
    /// Pixels of this frame per logical pixel of the view, as a Wayland
    /// buffer scale: 2.0 for a frame rendered for a 2x display. It is what
    /// decides the size the frame has when it is drawn unscaled.
    pub scale: f32,
    /// The producer's own number for this buffer, handed back untouched in
    /// [`FrameSource::release`]. A pool uses it as the slot to put back.
    ///
    /// [`FrameSource::release`]: super::FrameSource::release
    pub id: u64,
    /// Which size report this frame answers, when the producer numbers them.
    /// `None` (the default) says nothing. The view does not read it yet.
    pub resize_id: Option<u64>,
}

impl std::fmt::Debug for PixelFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PixelFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("stride", &self.stride)
            .field("format", &self.format)
            .field("damage", &self.damage)
            .field("y_inverted", &self.y_inverted)
            .field("scale", &self.scale)
            .field("id", &self.id)
            .field("resize_id", &self.resize_id)
            .finish_non_exhaustive()
    }
}

impl PixelFrame {
    /// A whole frame at scale 1 that is not inverted, with id 0. Set the other
    /// fields on the result.
    pub fn new(
        width: u32,
        height: u32,
        stride: usize,
        format: PixelFormat,
        bytes: PixelBytes,
    ) -> Result<Self, FrameError> {
        if width == 0 || height == 0 {
            return Err(FrameError::Empty);
        }
        let row = width as usize * format.bytes_per_pixel();
        if stride < row {
            return Err(FrameError::StrideTooSmall);
        }
        let needed = stride * (height as usize - 1) + row;
        if (*bytes).as_ref().len() < needed {
            return Err(FrameError::TooShort);
        }
        Ok(Self {
            width,
            height,
            stride,
            format,
            bytes,
            damage: None,
            y_inverted: false,
            scale: 1.0,
            id: 0,
            resize_id: None,
        })
    }

    pub fn with_damage(mut self, damage: DamageRect) -> Self {
        self.damage = Some(damage);
        self
    }

    pub fn with_y_inverted(mut self, y_inverted: bool) -> Self {
        self.y_inverted = y_inverted;
        self
    }

    /// Fails with [`FrameError::BadScale`] for a scale that is not above 0.
    pub fn with_scale(mut self, scale: f32) -> Result<Self, FrameError> {
        if !(scale > 0.0 && scale.is_finite()) {
            return Err(FrameError::BadScale);
        }
        self.scale = scale;
        Ok(self)
    }

    pub fn with_id(mut self, id: u64) -> Self {
        self.id = id;
        self
    }

    /// Marks the frame as the answer to size report number `resize_id`.
    pub fn with_resize_id(mut self, resize_id: u64) -> Self {
        self.resize_id = Some(resize_id);
        self
    }

    /// The pixels, `stride` bytes to a row.
    pub fn pixels(&self) -> &[u8] {
        (*self.bytes).as_ref()
    }

    /// The damage clamped to the frame; the whole frame when there is none.
    pub fn damage_rect(&self) -> DamageRect {
        match self.damage {
            Some(damage) => damage.clamped(self.width, self.height),
            None => DamageRect::full(self.width, self.height),
        }
    }

    /// The size in the view's logical pixels the frame has drawn unscaled.
    pub fn logical_size(&self) -> (f32, f32) {
        (
            self.width as f32 / self.scale,
            self.height as f32 / self.scale,
        )
    }
}

/// One frame from a producer. `#[non_exhaustive]`: a GPU buffer variant will
/// be added, so match with a wildcard arm.
#[derive(Debug)]
#[non_exhaustive]
pub enum Frame {
    /// Pixels in memory the view reads in place.
    Pixels(PixelFrame),
}

impl From<PixelFrame> for Frame {
    fn from(frame: PixelFrame) -> Self {
        Self::Pixels(frame)
    }
}
