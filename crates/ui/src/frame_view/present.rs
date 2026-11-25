//! Showing frames: the [`Presenter`] that owns what is on screen, and the
//! painting of it.
//!
//! One frame is on screen at a time. Taking a frame makes it a Skia image
//! that points at the producer's own bytes, so nothing is copied to show it;
//! the only pass over the pixels is the one that draws them (a blit in the
//! software renderer, the texture upload on the GPU). A repaint that brings no
//! new frame draws the same image again, which costs no upload.

use std::sync::Arc;

use freya::engine::prelude::{
    AlphaType, BlendMode, Canvas, ClipOp, ColorType, Data, FilterMode, ISize, ImageInfo,
    MipmapMode, Paint as Pen, PaintStyle, SamplingOptions, SkImage, SkRect, raster_from_data,
};
use freya::prelude::Color;

use super::frame::{Frame, PixelFormat, PixelFrame};
use super::geometry::{DeviceRect, place};
use super::input::FrameFit;
use super::source::FrameSource;

/// A frame on screen with the image that reads it.
struct Shown {
    image: SkImage,
    frame: PixelFrame,
}

/// What [`Presenter::pump`] found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pump {
    /// The producer had no new frame.
    Nothing,
    /// A new frame is on screen: the view repaints.
    Shown,
    /// A new frame arrived with nothing changed in it: it was handed straight
    /// back and the picture stays as it is.
    Unchanged,
}

/// Takes frames from a source and keeps the one on screen. Lives on the UI
/// thread.
pub(crate) struct Presenter {
    source: Arc<dyn FrameSource>,
    shown: Option<Shown>,
}

impl Presenter {
    pub(crate) fn new(source: Arc<dyn FrameSource>) -> Self {
        Self {
            source,
            shown: None,
        }
    }

    /// The frame on screen, for the tests.
    #[cfg(test)]
    pub(crate) fn shown_frame(&self) -> Option<&PixelFrame> {
        self.shown.as_ref().map(|shown| &shown.frame)
    }

    /// Asks the source for its newest frame and puts it on screen, giving
    /// back the frame it replaces. Called when the source wakes the view,
    /// never from a paint.
    pub(crate) fn pump(&mut self) -> Pump {
        let Some(frame) = self.source.take_frame() else {
            return Pump::Nothing;
        };
        match frame {
            Frame::Pixels(frame) => self.show(frame),
        }
    }

    fn show(&mut self, frame: PixelFrame) -> Pump {
        if let (Some(shown), Some(damage)) = (&self.shown, frame.damage)
            && damage.clamped(frame.width, frame.height).is_empty()
            && same_layout(&shown.frame, &frame)
        {
            self.source.release(Frame::Pixels(frame));
            return Pump::Unchanged;
        }
        let Some(image) = image_of(&frame) else {
            // The bytes do not describe an image Skia accepts: the producer
            // has its buffer back and the picture stays.
            self.source.release(Frame::Pixels(frame));
            return Pump::Nothing;
        };
        if let Some(old) = self.shown.replace(Shown { image, frame }) {
            // The old image was last drawn by an earlier paint, and a paint
            // ends by flushing: its bytes are no longer read.
            let Shown { image, frame } = old;
            drop(image);
            self.source.release(Frame::Pixels(frame));
        }
        Pump::Shown
    }

    /// Paints the view: the frame placed by `fit`, or the placeholder while
    /// there is none. The canvas has the view's top left as its origin and
    /// logical pixels as its unit; `size` is the view's size in them.
    pub(crate) fn paint(&self, canvas: &Canvas, size: (f32, f32), fit: FrameFit, look: &Look) {
        let Some(shown) = &self.shown else {
            fill(canvas, SkRect::from_wh(size.0, size.1), look.placeholder);
            return;
        };
        let matrix = canvas.local_to_device_as_3x3();
        if matrix.skew_x() != 0.0 || matrix.skew_y() != 0.0 || matrix.scale_x() != matrix.scale_y()
        {
            // Rotated or sheared by an ancestor: no pixel grid to land on.
            fill(canvas, SkRect::from_wh(size.0, size.1), look.backdrop);
            draw_image(canvas, shown, SkRect::from_wh(size.0, size.1), false);
            return;
        }
        let scale = f64::from(matrix.scale_x());
        let placement = place(
            fit,
            (shown.frame.width, shown.frame.height),
            shown.frame.scale,
            (matrix.translate_x(), matrix.translate_y()),
            size,
            scale,
        );
        let started = super::stats::timing().then(std::time::Instant::now);
        canvas.save();
        canvas.reset_matrix();
        canvas.clip_rect(to_sk(&placement.bounds), ClipOp::Intersect, false);
        if placement.dest != placement.bounds {
            // Anything the frame leaves bare shows the backdrop. A frame that
            // covers the view is drawn straight onto what is beneath it, so
            // its transparency shows the surface it sits on.
            fill(canvas, to_sk(&placement.bounds), look.backdrop);
        }
        draw_image(canvas, shown, to_sk(&placement.dest), placement.exact);
        canvas.restore();
        if let Some(started) = started {
            super::stats::painted(started.elapsed());
        }
    }
}

impl Drop for Presenter {
    fn drop(&mut self) {
        if let Some(Shown { image, frame }) = self.shown.take() {
            drop(image);
            self.source.release(Frame::Pixels(frame));
        }
    }
}

/// The colours the view paints with, from its tokens.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Look {
    /// Behind the frame where it does not reach.
    pub backdrop: Color,
    /// The whole view until the first frame.
    pub placeholder: Color,
}

fn to_sk(rect: &DeviceRect) -> SkRect {
    SkRect::new(rect.left, rect.top, rect.right, rect.bottom)
}

fn fill(canvas: &Canvas, rect: SkRect, colour: Color) {
    let mut pen = Pen::default();
    pen.set_style(PaintStyle::Fill);
    pen.set_color(colour);
    canvas.draw_rect(rect, &pen);
}

fn draw_image(canvas: &Canvas, shown: &Shown, dest: SkRect, exact: bool) {
    let mut pen = Pen::default();
    pen.set_anti_alias(false);
    let flipped = shown.frame.y_inverted;
    if flipped {
        canvas.save();
        canvas.translate((0.0, dest.top + dest.bottom));
        canvas.scale((1.0, -1.0));
    }
    // A frame the size of its place is copied pixel for pixel. Anything else
    // is filtered bilinearly: the producer is asked to render at the view's
    // size, so this is the catch-up while it does.
    let sampling = if exact {
        SamplingOptions::new(FilterMode::Nearest, MipmapMode::None)
    } else {
        SamplingOptions::new(FilterMode::Linear, MipmapMode::None)
    };
    if shown.frame.format == PixelFormat::Bgrx8 {
        pen.set_blend_mode(BlendMode::Src);
    }
    canvas.draw_image_rect_with_sampling_options(&shown.image, None, dest, sampling, &pen);
    if flipped {
        canvas.restore();
    }
}

/// Whether two frames are the same size and format, so the damage of the
/// second is meaningful against the first.
fn same_layout(a: &PixelFrame, b: &PixelFrame) -> bool {
    a.width == b.width
        && a.height == b.height
        && a.format == b.format
        && a.y_inverted == b.y_inverted
}

/// A Skia image that reads the frame's bytes in place.
fn image_of(frame: &PixelFrame) -> Option<SkImage> {
    let alpha = match frame.format {
        PixelFormat::Bgra8Premultiplied => AlphaType::Premul,
        PixelFormat::Bgrx8 => AlphaType::Opaque,
    };
    let info = ImageInfo::new(
        ISize::new(
            i32::try_from(frame.width).ok()?,
            i32::try_from(frame.height).ok()?,
        ),
        ColorType::BGRA8888,
        alpha,
        None,
    );
    let used = frame.stride * (frame.height as usize - 1)
        + frame.width as usize * frame.format.bytes_per_pixel();
    let bytes = frame.pixels().get(..used)?;
    // SAFETY: `Data::new_bytes` borrows `bytes` without copying and without a
    // lifetime. The memory belongs to `frame.bytes`, an `Arc` that `Shown`
    // keeps next to the image, and that the producer does not write to until
    // `release`. Skia may hold the image a little longer (a texture upload
    // still queued), but only until the flush that ends the paint that drew
    // it, and the frame is released after that (see `show`).
    let data = unsafe { Data::new_bytes(bytes) };
    raster_from_data(&info, data, frame.stride)
}
