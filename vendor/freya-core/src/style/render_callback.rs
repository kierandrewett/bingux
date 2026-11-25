use std::{
    cell::RefCell,
    fmt,
    hash::{
        Hash,
        Hasher,
    },
    rc::Rc,
};

use freya_engine::prelude::{
    Canvas,
    FontCollection,
};
use torin::prelude::{
    Point2D,
    Size2D,
};

use crate::data::TextStyleState;

/// Context for drawing a rect background or overlaid text.
pub struct RenderContext<'a> {
    pub canvas: &'a Canvas,
    pub font_collection: &'a mut FontCollection,
    pub origin: Point2D,
    pub size: Size2D,
    pub text_style_state: &'a TextStyleState,
}

type Callback = Rc<RefCell<dyn FnMut(&mut RenderContext)>>;

#[derive(Clone)]
pub struct RenderCallback {
    callback: Callback,
    snap_to_pixels: bool,
}

impl RenderCallback {
    pub fn new(callback: impl FnMut(&mut RenderContext) + 'static) -> Self {
        Self {
            callback: Rc::new(RefCell::new(callback)),
            snap_to_pixels: false,
        }
    }

    /// Bingux patch: the box that paints with this callback is drawn on whole
    /// device pixels: its corner and its size are rounded, the clip it draws
    /// inside follows, and the callback is given the rounded box. Without it a
    /// box at a fractional position (at a 1.25x display most are) has a clip
    /// that cuts its edge pixels in half, which spoils a picture that is
    /// supposed to land pixel for pixel.
    pub fn snapped_to_pixels(mut self) -> Self {
        self.snap_to_pixels = true;
        self
    }

    /// Whether the box is drawn on whole device pixels; see
    /// [`RenderCallback::snapped_to_pixels`].
    pub fn snaps_to_pixels(&self) -> bool {
        self.snap_to_pixels
    }

    pub fn call(&self, context: &mut RenderContext) {
        (self.callback.borrow_mut())(context);
    }
}

impl Default for RenderCallback {
    fn default() -> Self {
        Self::new(|_| {})
    }
}

impl<F: FnMut(&mut RenderContext) + 'static> From<F> for RenderCallback {
    fn from(callback: F) -> Self {
        Self::new(callback)
    }
}

impl PartialEq for RenderCallback {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.callback, &other.callback) && self.snap_to_pixels == other.snap_to_pixels
    }
}

impl Hash for RenderCallback {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.callback).hash(state);
    }
}

impl fmt::Debug for RenderCallback {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("RenderCallback").finish_non_exhaustive()
    }
}

impl fmt::Display for RenderCallback {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "RenderCallback({:p})", Rc::as_ptr(&self.callback))
    }
}
