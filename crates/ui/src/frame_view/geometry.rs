//! The maths of the view: where a frame lands in the window's pixels, and
//! when a size change is worth telling the producer. Pure functions, so the
//! tests check exactly what is drawn and reported.

use std::time::{Duration, Instant};

use super::input::{FrameFit, ViewSize};

/// A rectangle in device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeviceRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl DeviceRect {
    pub(crate) fn new(left: f32, top: f32, width: f32, height: f32) -> Self {
        Self {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }

    pub(crate) fn width(&self) -> f32 {
        self.right - self.left
    }

    pub(crate) fn height(&self) -> f32 {
        self.bottom - self.top
    }
}

/// The device pixels a logical length takes, to a whole pixel: what the view
/// is drawn at, and what [`ViewSize::device_size`] tells a producer to render
/// at, so the two cannot disagree by a pixel.
pub(crate) fn device_extent(logical: f32, scale: f64) -> f32 {
    (f64::from(logical) * scale + SNAP_BIAS).floor().max(0.0) as f32
}

/// Rounding to the pixel grid leans a little upward (a thousandth of a pixel),
/// so a box that sits exactly between two pixels, as a 1.25x layout often
/// puts one, rounds the same way whatever float noise the layout left on it.
/// The vendored renderer rounds a pixel-snapped box with the same bias.
const SNAP_BIAS: f64 = 0.501;

/// The nearest whole device pixel to `value`; see [`SNAP_BIAS`].
pub(crate) fn snap(value: f32) -> f32 {
    (f64::from(value) + SNAP_BIAS).floor() as f32
}

/// Where and how one frame is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placement {
    /// Where the frame's pixels go, on whole device pixels.
    pub dest: DeviceRect,
    /// The view's own box on the pixel grid: drawing is clipped to it.
    pub bounds: DeviceRect,
    /// The frame is exactly the size of `dest`: every source pixel lands on
    /// one device pixel, so it is drawn without filtering and does not blur.
    pub exact: bool,
}

/// Places a `frame` (pixels, drawn from a buffer of `frame_scale`) in a view
/// whose top left is at `origin` device pixels and whose size is `logical`
/// logical pixels, at `view_scale` device pixels per logical one.
///
/// The view's box lands on the pixel grid: its corner on the nearest device
/// pixel and its size [`device_extent`]. A layout box can sit on a fraction of
/// a pixel (at 1.25x it often does); a frame of the box's size is then copied,
/// not resampled, and a producer that renders at [`ViewSize::device_size`] is
/// always exact.
pub(crate) fn place(
    fit: FrameFit,
    frame: (u32, u32),
    frame_scale: f32,
    origin: (f32, f32),
    logical: (f32, f32),
    view_scale: f64,
) -> Placement {
    let bounds = DeviceRect::new(
        snap(origin.0),
        snap(origin.1),
        device_extent(logical.0, view_scale),
        device_extent(logical.1, view_scale),
    );
    let (fw, fh) = (frame.0 as f32, frame.1 as f32);
    let dest = match fit {
        FrameFit::Stretch => bounds,
        FrameFit::Anchored => {
            let ratio = (view_scale / f64::from(frame_scale)) as f32;
            DeviceRect::new(
                bounds.left,
                bounds.top,
                (fw * ratio).round(),
                (fh * ratio).round(),
            )
        }
        FrameFit::Contain => {
            let ratio = (bounds.width() / fw).min(bounds.height() / fh);
            let (width, height) = ((fw * ratio).round(), (fh * ratio).round());
            DeviceRect::new(
                bounds.left + ((bounds.width() - width) / 2.0).round(),
                bounds.top + ((bounds.height() - height) / 2.0).round(),
                width,
                height,
            )
        }
    };
    let exact = dest.width() == fw && dest.height() == fh;
    Placement {
        dest,
        bounds,
        exact,
    }
}

/// How often a size is reported at most: once per frame of the 60 Hz clock.
pub(crate) const REPORT_EVERY: Duration = Duration::from_millis(16);

/// What to do with a size the layout just measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Report {
    /// Tell the producer now.
    Now(ViewSize),
    /// Too soon after the last report: ask again after this long, once.
    After(Duration),
    /// Nothing to do: the size is the one already told, or a timer is armed.
    Nothing,
}

/// Debounces the size reports to once per [`REPORT_EVERY`], with the first
/// one at once and the last one always delivered. It never reports the size
/// it last reported, so a producer that answers a size with a frame cannot
/// start a loop: the answer does not change the view's box, and if it did,
/// the box would settle on one size and stop.
#[derive(Clone, Debug, Default)]
pub(crate) struct SizeReporter {
    sent: Option<(ViewSize, Instant)>,
    latest: Option<ViewSize>,
    armed: bool,
}

impl SizeReporter {
    pub(crate) fn offer(&mut self, size: ViewSize, now: Instant) -> Report {
        self.latest = Some(size);
        let Some((sent, at)) = self.sent else {
            return self.deliver(size, now);
        };
        if self.armed {
            return Report::Nothing;
        }
        if sent.same_as(&size) {
            return Report::Nothing;
        }
        let since = now.saturating_duration_since(at);
        if since >= REPORT_EVERY {
            return self.deliver(size, now);
        }
        self.armed = true;
        Report::After(REPORT_EVERY - since)
    }

    /// The timer of a [`Report::After`] ran out: the latest size, unless it is
    /// the one already reported.
    pub(crate) fn fire(&mut self, now: Instant) -> Option<ViewSize> {
        self.armed = false;
        let latest = self.latest?;
        if self.sent.is_some_and(|(sent, _)| sent.same_as(&latest)) {
            return None;
        }
        self.sent = Some((latest, now));
        Some(latest)
    }

    fn deliver(&mut self, size: ViewSize, now: Instant) -> Report {
        self.sent = Some((size, now));
        Report::Now(size)
    }
}
