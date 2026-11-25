//! Tests of the frame view's parts that need no window: frames, selection of
//! the newest, the placement maths, drawing onto a raster surface at the
//! scales a display has, the size debounce, the input maths and the producer.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use freya::engine::prelude::{ImageInfo, Surface, raster_n32_premul};
use freya::prelude::*;

use super::frame::{DamageRect, Frame, FrameError, PixelBytes, PixelFormat, PixelFrame};
use super::geometry::{REPORT_EVERY, Report, SizeReporter, device_extent, place, snap};
use super::input::{
    ClickCounter, FocusEscape, FrameFit, KeyRoute, KeyTracker, MULTI_CLICK, PointerButtons,
    TextInput, ViewSize, WheelDelta, WheelScaling, relative, wheel_delta,
};
use super::pattern::{self, PATTERN_MIN_WIDTH, PatternSource};
use super::present::{Look, Presenter, Pump};
use super::source::{FrameSource, WakeSignal};
use crate::style::Paint;
use crate::theme::{DEFAULT_ACCENT, Scheme, Tokens};

const SCALES: [f64; 3] = [1.0, 1.25, 2.0];

fn tokens() -> Tokens {
    Tokens::new(DEFAULT_ACCENT, Scheme::PreferDark, false)
}

fn look() -> Look {
    let tokens = tokens();
    Look {
        backdrop: Paint::Backdrop.color(&tokens),
        placeholder: Paint::Control.color(&tokens),
    }
}

/// A colour as the BGRA bytes a raster surface holds.
fn bgra(colour: Color) -> [u8; 4] {
    [colour.b(), colour.g(), colour.r(), colour.a()]
}

fn bytes(data: Vec<u8>) -> PixelBytes {
    Arc::new(data)
}

fn solid(width: u32, height: u32, id: u64, colour: [u8; 4]) -> PixelFrame {
    let data: Vec<u8> = colour
        .iter()
        .copied()
        .cycle()
        .take(width as usize * height as usize * 4)
        .collect();
    PixelFrame::new(
        width,
        height,
        width as usize * 4,
        PixelFormat::Bgra8Premultiplied,
        bytes(data),
    )
    .expect("a valid frame")
    .with_id(id)
}

/// A source that hands out the frames it was given, in order, and records the
/// ids it gets back.
#[derive(Default)]
struct Scripted {
    frames: Mutex<VecDeque<PixelFrame>>,
    released: Mutex<Vec<u64>>,
}

impl Scripted {
    fn with(frames: Vec<PixelFrame>) -> Arc<Self> {
        Arc::new(Self {
            frames: Mutex::new(frames.into()),
            released: Mutex::default(),
        })
    }

    fn released(&self) -> Vec<u64> {
        self.released.lock().unwrap().clone()
    }
}

impl FrameSource for Scripted {
    fn take_frame(&self) -> Option<Frame> {
        self.frames.lock().unwrap().pop_front().map(Frame::Pixels)
    }

    fn set_waker(&self, _: Waker) {}

    fn release(&self, frame: Frame) {
        let Frame::Pixels(frame) = frame;
        self.released.lock().unwrap().push(frame.id);
    }
}

/// A raster surface the size of a window, painted the way Freya paints a node:
/// translated to the node's box in device pixels, scaled by the display scale.
struct Rig {
    surface: Surface,
    width: i32,
    height: i32,
}

impl Rig {
    fn new(width: i32, height: i32) -> Self {
        Self {
            surface: raster_n32_premul((width, height)).expect("a surface"),
            width,
            height,
        }
    }

    fn paint(
        &mut self,
        presenter: &Presenter,
        origin: (f32, f32),
        size: (f32, f32),
        scale: f64,
        fit: FrameFit,
    ) {
        let canvas = self.surface.canvas();
        canvas.clear(Paint::Card.color(&tokens()));
        canvas.save();
        canvas.translate((origin.0 * scale as f32, origin.1 * scale as f32));
        canvas.scale((scale as f32, scale as f32));
        presenter.paint(canvas, size, fit, &look());
        canvas.restore();
    }

    fn pixels(&mut self) -> Vec<u8> {
        let info = ImageInfo::new_n32_premul((self.width, self.height), None);
        let mut out = vec![0u8; self.width as usize * self.height as usize * 4];
        assert!(
            self.surface
                .read_pixels(&info, &mut out, self.width as usize * 4, (0, 0))
        );
        out
    }

    fn at(&mut self, x: u32, y: u32) -> [u8; 4] {
        let pixels = self.pixels();
        let at = (y as usize * self.width as usize + x as usize) * 4;
        pixels[at..at + 4].try_into().unwrap()
    }
}

fn card() -> [u8; 4] {
    bgra(Paint::Card.color(&tokens()))
}

// ---- frames -----------------------------------------------------------------

#[test]
fn a_frame_is_checked_against_its_bytes() {
    let ok = |w, h, stride, len| {
        PixelFrame::new(w, h, stride, PixelFormat::Bgrx8, bytes(vec![0; len])).map(|_| ())
    };
    assert_eq!(ok(0, 4, 16, 64), Err(FrameError::Empty));
    assert_eq!(ok(4, 0, 16, 64), Err(FrameError::Empty));
    assert_eq!(ok(4, 4, 15, 64), Err(FrameError::StrideTooSmall));
    assert_eq!(ok(4, 4, 16, 63), Err(FrameError::TooShort));
    assert_eq!(ok(4, 4, 16, 64), Ok(()));
    // The last row needs only its own pixels, not a whole stride.
    assert_eq!(ok(4, 4, 24, 24 * 3 + 16), Ok(()));
    let scaled = |scale| solid(4, 4, 0, [0; 4]).with_scale(scale).map(|_| ());
    assert_eq!(scaled(0.0), Err(FrameError::BadScale));
    assert_eq!(scaled(f32::NAN), Err(FrameError::BadScale));
    assert_eq!(scaled(-1.0), Err(FrameError::BadScale));
    assert_eq!(scaled(2.0), Ok(()));
}

#[test]
fn damage_rects_fold_and_clamp() {
    let a = DamageRect::new(10, 10, 20, 20);
    let b = DamageRect::new(25, 5, 20, 10);
    assert_eq!(a.union(b), DamageRect::new(10, 5, 35, 25));
    assert_eq!(
        a.union(DamageRect::new(0, 0, 0, 40)),
        a,
        "an empty rectangle adds nothing"
    );
    assert_eq!(DamageRect::new(0, 0, 0, 0).union(b), b);
    assert_eq!(
        DamageRect::new(90, 90, 50, 50).clamped(100, 100),
        DamageRect::new(90, 90, 10, 10)
    );
    assert!(DamageRect::new(120, 0, 5, 5).clamped(100, 100).is_empty());
    assert_eq!(
        DamageRect::new(u32::MAX - 1, 0, 10, 4)
            .clamped(100, 100)
            .width,
        0,
        "no overflow"
    );
}

// ---- which frame is shown ---------------------------------------------------

#[test]
fn the_newest_frame_wins_and_every_frame_is_released_once() {
    let source = PatternSource::stepped(160, 90);
    let mut presenter = Presenter::new(source.clone());
    assert_eq!(presenter.pump(), Pump::Nothing, "nothing drawn yet");

    // Three frames arrive before the view looks: it sees only the last.
    source.step();
    source.step();
    source.step();
    assert_eq!(presenter.pump(), Pump::Shown);
    let shown = presenter.shown_frame().unwrap();
    assert_eq!(
        PatternSource::frame_number(shown),
        Some(2),
        "frames 0 and 1 were skipped"
    );
    let stats = source.stats();
    assert_eq!(
        (stats.rendered, stats.taken, stats.dropped, stats.released),
        (3, 1, 2, 0)
    );
    assert_eq!(
        presenter.pump(),
        Pump::Nothing,
        "the same frame is not shown twice"
    );

    // The next frame replaces it, and the replaced one goes back.
    source.step();
    assert_eq!(presenter.pump(), Pump::Shown);
    assert_eq!(
        PatternSource::frame_number(presenter.shown_frame().unwrap()),
        Some(3)
    );
    assert_eq!(source.stats().released, 1);

    // Unmounting gives back the one on screen: taken == released, no stray.
    drop(presenter);
    let stats = source.stats();
    assert_eq!(
        (stats.taken, stats.released, stats.stray_releases),
        (2, 2, 0)
    );
}

#[test]
fn a_producer_that_draws_faster_than_the_view_never_runs_out_of_buffers() {
    let source = PatternSource::stepped(64, 64);
    let mut presenter = Presenter::new(source.clone());
    for round in 0..50u64 {
        for _ in 0..(round % 5 + 1) {
            source.step();
        }
        assert_eq!(presenter.pump(), Pump::Shown);
    }
    let stats = source.stats();
    assert_eq!(stats.taken, 50);
    assert_eq!(stats.released, 49, "the one on screen is still out");
    assert_eq!(stats.stray_releases, 0);
    assert_eq!(
        stats.rendered,
        50 + stats.dropped,
        "every frame is shown or dropped"
    );
}

#[test]
fn a_frame_with_nothing_changed_is_given_back_and_the_picture_stays() {
    let source = Scripted::with(vec![
        solid(8, 8, 1, [10, 20, 30, 255]),
        solid(8, 8, 2, [99, 99, 99, 255]).with_damage(DamageRect::new(0, 0, 0, 0)),
        solid(8, 8, 3, [40, 50, 60, 255]).with_damage(DamageRect::new(100, 100, 4, 4)),
    ]);
    let mut presenter = Presenter::new(source.clone());
    assert_eq!(presenter.pump(), Pump::Shown);
    // Empty damage: unchanged, and the buffer is back at once.
    assert_eq!(presenter.pump(), Pump::Unchanged);
    assert_eq!(presenter.shown_frame().unwrap().id, 1);
    assert_eq!(source.released(), vec![2]);
    // Damage entirely outside the frame is empty once clamped.
    assert_eq!(presenter.pump(), Pump::Unchanged);
    assert_eq!(source.released(), vec![2, 3]);
    drop(presenter);
    assert_eq!(
        source.released(),
        vec![2, 3, 1],
        "each frame came back exactly once"
    );
}

#[test]
fn damage_means_nothing_when_the_size_changed() {
    let source = Scripted::with(vec![
        solid(8, 8, 1, [1, 2, 3, 255]),
        solid(16, 8, 2, [4, 5, 6, 255]).with_damage(DamageRect::new(0, 0, 0, 0)),
    ]);
    let mut presenter = Presenter::new(source.clone());
    assert_eq!(presenter.pump(), Pump::Shown);
    assert_eq!(presenter.pump(), Pump::Shown, "a new size always shows");
    assert_eq!(presenter.shown_frame().unwrap().id, 2);
    assert_eq!(source.released(), vec![1]);
}

#[test]
fn a_partial_damage_frame_replaces_the_picture() {
    // Freya repaints the whole tree, so a partial rectangle cannot shrink the
    // paint: the frame is shown whole.
    let source = Scripted::with(vec![
        solid(8, 8, 1, [1, 2, 3, 255]),
        solid(8, 8, 2, [4, 5, 6, 255]).with_damage(DamageRect::new(2, 2, 3, 3)),
    ]);
    let mut presenter = Presenter::new(source.clone());
    presenter.pump();
    assert_eq!(presenter.pump(), Pump::Shown);
    assert_eq!(
        presenter.shown_frame().unwrap().damage_rect(),
        DamageRect::new(2, 2, 3, 3)
    );
    assert_eq!(source.released(), vec![1]);
}

// ---- where the frame lands ----------------------------------------------------

#[test]
fn a_frame_of_the_boxs_device_size_is_exact_at_every_scale_and_origin() {
    for scale in SCALES {
        for origin in [
            (0.0, 0.0),
            (10.4, 7.2),
            (3.3, 9.9),
            (100.5, 0.5),
            (0.1, 0.9),
        ] {
            for logical in [(80.0, 60.0), (80.4, 60.6), (81.0, 61.0), (333.3, 211.7)] {
                let size = ViewSize {
                    width: logical.0,
                    height: logical.1,
                    scale,
                };
                let (w, h) = size.device_size();
                let device_origin = (origin.0 * scale as f32, origin.1 * scale as f32);
                let placed = place(
                    FrameFit::Stretch,
                    (w, h),
                    scale as f32,
                    device_origin,
                    logical,
                    scale,
                );
                assert!(
                    placed.exact,
                    "{scale}x {origin:?} {logical:?}: a {w}x{h} frame must be 1:1"
                );
                assert_eq!(placed.dest, placed.bounds);
                assert_eq!(
                    placed.dest.left,
                    placed.dest.left.floor(),
                    "on a whole device pixel"
                );
                assert_eq!(placed.dest.top, placed.dest.top.floor());
                assert_eq!(
                    (placed.dest.width(), placed.dest.height()),
                    (w as f32, h as f32)
                );

                // One pixel short is a stretch, not an exact copy.
                let short = place(
                    FrameFit::Stretch,
                    (w - 1, h),
                    scale as f32,
                    device_origin,
                    logical,
                    scale,
                );
                assert!(!short.exact);
            }
        }
    }
}

#[test]
fn device_size_rounds_as_the_view_does() {
    let size = ViewSize {
        width: 80.4,
        height: 60.6,
        scale: 1.25,
    };
    assert_eq!(size.device_size(), (101, 76));
    assert_eq!(device_extent(80.4, 1.25), 101.0);
    assert_eq!(
        ViewSize {
            width: 400.0,
            height: 300.0,
            scale: 2.0
        }
        .device_size(),
        (800, 600)
    );
    assert_eq!(size.to_device((10.0, 4.0)), (12.5, 5.0));
}

#[test]
fn a_frame_can_sit_at_its_own_size_or_inside_the_box() {
    // 200x100 pixels rendered for a 2x display, in a view that is 2x: exact.
    let at_2x = place(
        FrameFit::Anchored,
        (200, 100),
        2.0,
        (20.0, 10.0),
        (150.0, 90.0),
        2.0,
    );
    assert!(at_2x.exact);
    assert_eq!(
        (
            at_2x.dest.left,
            at_2x.dest.top,
            at_2x.dest.width(),
            at_2x.dest.height()
        ),
        (20.0, 10.0, 200.0, 100.0)
    );
    assert_eq!(
        at_2x.bounds.width(),
        300.0,
        "the clip is the view, not the frame"
    );
    // The same frame on a 1x display is half its pixel size, so it is scaled.
    let at_1x = place(
        FrameFit::Anchored,
        (200, 100),
        2.0,
        (20.0, 10.0),
        (150.0, 90.0),
        1.0,
    );
    assert!(!at_1x.exact);
    assert_eq!((at_1x.dest.width(), at_1x.dest.height()), (100.0, 50.0));

    // 16:9 in a square keeps its ratio and is centred.
    let contained = place(
        FrameFit::Contain,
        (1920, 1080),
        1.0,
        (0.0, 0.0),
        (400.0, 400.0),
        1.0,
    );
    assert_eq!(
        (contained.dest.width(), contained.dest.height()),
        (400.0, 225.0)
    );
    assert_eq!(contained.dest.top, 88.0);
    assert!(!contained.exact);
    // And it is exact when it fits to the pixel.
    assert!(
        place(
            FrameFit::Contain,
            (400, 225),
            1.0,
            (0.0, 0.0),
            (400.0, 400.0),
            1.0
        )
        .exact
    );
}

fn shown(source: &Arc<PatternSource>) -> Presenter {
    let mut presenter = Presenter::new(source.clone());
    source.step();
    assert_eq!(presenter.pump(), Pump::Shown);
    presenter
}

#[test]
fn an_exact_frame_is_copied_pixel_for_pixel_at_every_scale() {
    for scale in SCALES {
        let origin = (10.4, 7.2);
        let logical = (81.0, 61.0);
        let (w, h) = ViewSize {
            width: logical.0,
            height: logical.1,
            scale,
        }
        .device_size();
        let source = PatternSource::stepped(w, h);
        let presenter = shown(&source);
        let mut rig = Rig::new(400, 300);
        rig.paint(&presenter, origin, logical, scale, FrameFit::Stretch);
        let pixels = rig.pixels();
        let frame = presenter.shown_frame().unwrap();
        let left = snap(origin.0 * scale as f32) as usize;
        let top = snap(origin.1 * scale as f32) as usize;
        for row in 0..h as usize {
            let from = ((top + row) * 400 + left) * 4;
            let on_screen = &pixels[from..from + w as usize * 4];
            let in_frame = &frame.pixels()[row * frame.stride..row * frame.stride + w as usize * 4];
            assert_eq!(on_screen, in_frame, "{scale}x: row {row} was resampled");
        }
        // Nothing leaks past the box.
        for (x, y) in [
            (left - 1, top),
            (left + w as usize, top),
            (left, top - 1),
            (left, top + h as usize),
        ] {
            let at = (y * 400 + x) * 4;
            assert_eq!(
                pixels[at..at + 4],
                card(),
                "{scale}x: ({x},{y}) is outside the view"
            );
        }
    }
}

#[test]
fn a_frame_of_another_size_is_stretched_and_blends_neighbours() {
    let source = PatternSource::stepped(64, 64);
    let presenter = shown(&source);
    let mut rig = Rig::new(200, 200);
    rig.paint(
        &presenter,
        (0.0, 0.0),
        (100.0, 100.0),
        1.0,
        FrameFit::Stretch,
    );
    // The corner marker is red, and the border white, at 2x: the white
    // border now covers two pixels and the picture reaches the box's edge.
    assert_eq!(rig.at(0, 0), pattern_colour::BORDER);
    assert_eq!(rig.at(99, 99), pattern_colour::BORDER);
    assert_eq!(rig.at(100, 100), card());
    // Along a row of the checkerboard the stretch blends the light and the
    // dark square where they meet: a value strictly between the two.
    let row: Vec<u8> = (0..100).map(|x| rig.at(x, 30)[0]).collect();
    let (dark, light) = (pattern_colour::DARK[0], pattern_colour::LIGHT[0]);
    assert!(
        row.iter().any(|&b| b > dark && b < light),
        "a stretched frame is filtered: {row:?}"
    );
}

/// The pattern's colours, as the tests read them back.
mod pattern_colour {
    pub const BORDER: [u8; 4] = [255, 255, 255, 255];
    pub const RED: [u8; 4] = [40, 40, 230, 255];
    pub const GREEN: [u8; 4] = [60, 200, 60, 255];
    pub const BLUE: [u8; 4] = [230, 90, 40, 255];
    pub const YELLOW: [u8; 4] = [40, 220, 230, 255];
    pub const DARK: [u8; 4] = [38, 36, 36, 255];
    pub const LIGHT: [u8; 4] = [58, 56, 56, 255];
}

#[test]
fn corners_show_a_flip_or_a_shift() {
    for y_inverted in [false, true] {
        let source = PatternSource::stepped(100, 80);
        source.set_y_inverted(y_inverted);
        let presenter = shown(&source);
        assert_eq!(presenter.shown_frame().unwrap().y_inverted, y_inverted);
        let mut rig = Rig::new(120, 100);
        rig.paint(
            &presenter,
            (5.0, 5.0),
            (100.0, 80.0),
            1.0,
            FrameFit::Stretch,
        );
        // Upright on screen whichever way the rows are stored.
        assert_eq!(
            rig.at(5 + 3, 5 + 3),
            pattern_colour::RED,
            "top left, inverted: {y_inverted}"
        );
        assert_eq!(
            rig.at(5 + 96, 5 + 3),
            pattern_colour::GREEN,
            "top right, inverted: {y_inverted}"
        );
        assert_eq!(
            rig.at(5 + 3, 5 + 76),
            pattern_colour::BLUE,
            "bottom left, inverted: {y_inverted}"
        );
        assert_eq!(
            rig.at(5 + 96, 5 + 76),
            pattern_colour::YELLOW,
            "bottom right, inverted: {y_inverted}"
        );
        assert_eq!(rig.at(5, 5), pattern_colour::BORDER);
    }
}

#[test]
fn until_the_first_frame_the_view_shows_the_placeholder_colour() {
    let source = PatternSource::stepped(64, 64);
    let presenter = Presenter::new(source);
    let mut rig = Rig::new(100, 100);
    rig.paint(
        &presenter,
        (10.0, 10.0),
        (50.0, 40.0),
        1.0,
        FrameFit::Stretch,
    );
    let placeholder = bgra(look().placeholder);
    assert_eq!(rig.at(10, 10), placeholder);
    assert_eq!(rig.at(59, 49), placeholder);
    assert_eq!(rig.at(60, 50), card());
    assert_ne!(
        placeholder,
        bgra(look().backdrop),
        "the placeholder is not the letterbox colour"
    );
}

#[test]
fn a_frame_that_does_not_fill_the_view_leaves_the_backdrop_around_it() {
    let source = PatternSource::stepped(50, 30);
    let presenter = shown(&source);
    let mut rig = Rig::new(120, 100);
    let backdrop = bgra(look().backdrop);
    // Top left: the frame at its own size in the corner, backdrop to the right and below.
    rig.paint(
        &presenter,
        (10.0, 10.0),
        (100.0, 80.0),
        1.0,
        FrameFit::Anchored,
    );
    assert_eq!(rig.at(10, 10), pattern_colour::BORDER);
    assert_eq!(rig.at(59, 39), pattern_colour::BORDER);
    assert_eq!(rig.at(60, 40), backdrop);
    assert_eq!(rig.at(109, 89), backdrop);
    assert_eq!(rig.at(110, 90), card());
    // Contain: 5:3 in 100x80 is 100x60, centred.
    rig.paint(
        &presenter,
        (10.0, 10.0),
        (100.0, 80.0),
        1.0,
        FrameFit::Contain,
    );
    assert_eq!(rig.at(50, 15), backdrop, "the bar above the picture");
    assert_eq!(
        rig.at(50, 20),
        pattern_colour::BORDER,
        "the picture's top edge"
    );
    assert_eq!(rig.at(50, 85), backdrop, "the bar below it");
}

#[test]
fn a_resize_shows_the_old_frame_one_to_one_until_a_frame_of_the_new_size_arrives() {
    assert_eq!(FrameFit::default(), FrameFit::Anchored);
    let source = PatternSource::stepped(50, 30);
    let mut presenter = shown(&source);
    let mut rig = Rig::new(160, 120);
    let backdrop = bgra(look().backdrop);
    // The view grew to 100x80 and the producer has not answered: the frame is
    // not stretched, it keeps its size and the new area is the backdrop.
    rig.paint(&presenter, (10.0, 10.0), (100.0, 80.0), 1.0, FrameFit::default());
    assert_eq!(rig.at(10, 10), pattern_colour::BORDER);
    assert_eq!(rig.at(59, 39), pattern_colour::BORDER, "the old frame's last pixel");
    assert_eq!(rig.at(60, 39), backdrop, "right of it is newly exposed");
    assert_eq!(rig.at(59, 40), backdrop, "below it is newly exposed");
    assert_eq!(rig.at(109, 89), backdrop);
    // The view shrank below the frame: it is clipped, still not scaled.
    rig.paint(&presenter, (10.0, 10.0), (30.0, 20.0), 1.0, FrameFit::default());
    assert_ne!(rig.at(39, 29), pattern_colour::BORDER, "the frame's corner is clipped away, not scaled into view");
    assert_ne!(rig.at(39, 29), card(), "the view is still covered by the frame");
    assert_eq!(rig.at(40, 29), card(), "nothing is drawn outside the view");
    assert_eq!(rig.at(39, 30), card());
    // The producer answers with a frame of the new size: it fills the view.
    source.resize(100, 80, 1.0);
    source.step();
    assert_eq!(presenter.pump(), Pump::Shown);
    rig.paint(&presenter, (10.0, 10.0), (100.0, 80.0), 1.0, FrameFit::default());
    assert_eq!(rig.at(109, 89), pattern_colour::BORDER, "the new frame reaches the corner");
    assert_eq!(rig.at(110, 90), card());
}

#[test]
fn an_opaque_frame_replaces_and_one_with_alpha_blends() {
    let make = |format, alpha: u8| {
        let mut data = vec![0u8; 4 * 4 * 4];
        for pixel in data.chunks_exact_mut(4) {
            // A premultiplied half transparent white, whatever the format says.
            pixel.copy_from_slice(&[alpha, alpha, alpha, alpha]);
        }
        Scripted::with(vec![
            PixelFrame::new(4, 4, 16, format, bytes(data)).unwrap(),
        ])
    };
    let mut rig = Rig::new(8, 8);
    let mut presenter = Presenter::new(make(PixelFormat::Bgra8Premultiplied, 128));
    presenter.pump();
    rig.paint(&presenter, (0.0, 0.0), (4.0, 4.0), 1.0, FrameFit::Stretch);
    let blended = rig.at(1, 1);
    let under = card();
    assert!(
        blended[0] > under[0] && blended[0] < 255,
        "alpha lets the surface under it show: {blended:?}"
    );

    let mut presenter = Presenter::new(make(PixelFormat::Bgrx8, 128));
    presenter.pump();
    rig.paint(&presenter, (0.0, 0.0), (4.0, 4.0), 1.0, FrameFit::Stretch);
    assert_eq!(
        rig.at(1, 1)[..3],
        [128, 128, 128],
        "the fourth byte of Bgrx8 is ignored"
    );
}

// ---- input maths --------------------------------------------------------------

#[test]
fn pointer_positions_are_in_the_views_logical_pixels_at_every_scale() {
    // Freya hands events over in logical pixels; the view subtracts its origin.
    for scale in SCALES {
        let origin = (10.4_f32, 7.2_f32);
        let global = (50.4_f64, 27.2_f64);
        let position = relative(global, origin);
        assert!(
            (position.0 - 40.0).abs() < 1e-4 && (position.1 - 20.0).abs() < 1e-4,
            "{scale}x: {position:?}"
        );
        let size = ViewSize {
            width: 81.0,
            height: 61.0,
            scale,
        };
        let device = size.to_device(position);
        assert!((f64::from(device.0) - 40.0 * scale).abs() < 1e-3);
        assert!((f64::from(device.1) - 20.0 * scale).abs() < 1e-3);
        // Outside to the left and above is negative, which a drag out of the
        // view reports.
        let outside = relative((5.0, 2.0), origin);
        assert!(outside.0 < 0.0 && outside.1 < 0.0);
    }
}

#[test]
fn a_pointer_names_the_frame_pixel_under_it_to_within_half_a_pixel() {
    // The view's corner sits on the nearest device pixel, so the position that
    // names a frame pixel can be off by half a pixel from the box's own corner:
    // that is the whole error of mapping an event to the frame.
    for scale in SCALES {
        for origin in [(10.4_f32, 7.2_f32), (0.0, 0.0), (33.3, 9.9)] {
            let logical = (81.0_f32, 61.0_f32);
            let size = ViewSize {
                width: logical.0,
                height: logical.1,
                scale,
            };
            let (w, h) = size.device_size();
            let device_origin = (origin.0 * scale as f32, origin.1 * scale as f32);
            let placed = place(
                FrameFit::Stretch,
                (w, h),
                scale as f32,
                device_origin,
                logical,
                scale,
            );
            for (px, py) in [(0u32, 0u32), (w / 2, h / 2), (w - 1, h - 1)] {
                // The cursor is at the centre of device pixel (px, py) of the box.
                let cursor = (
                    placed.dest.left + px as f32 + 0.5,
                    placed.dest.top + py as f32 + 0.5,
                );
                let global = (f64::from(cursor.0) / scale, f64::from(cursor.1) / scale);
                let position = relative(global, origin);
                let mapped = size.to_device(position);
                // The producer's frame pixel is floor(mapped); the true one is (px, py).
                assert!(
                    (mapped.0 - (px as f32 + 0.5)).abs() <= 0.5 + 1e-3,
                    "{scale}x {origin:?} x: {mapped:?} vs {px}"
                );
                assert!(
                    (mapped.1 - (py as f32 + 0.5)).abs() <= 0.5 + 1e-3,
                    "{scale}x {origin:?} y: {mapped:?} vs {py}"
                );
            }
        }
    }
}

#[test]
fn clicks_count_while_they_are_close_in_time_and_place() {
    let mut counter = ClickCounter::default();
    let t = Instant::now();
    assert_eq!(counter.press(t, MouseButton::Left, (10.0, 10.0)), 1);
    assert_eq!(
        counter.press(
            t + Duration::from_millis(200),
            MouseButton::Left,
            (12.0, 11.0)
        ),
        2
    );
    assert_eq!(
        counter.press(
            t + Duration::from_millis(400),
            MouseButton::Left,
            (12.0, 11.0)
        ),
        3
    );
    assert_eq!(
        counter.current(),
        3,
        "the release carries the count of its press"
    );
    assert_eq!(
        counter.press(
            t + Duration::from_millis(400) + MULTI_CLICK + Duration::from_millis(1),
            MouseButton::Left,
            (12.0, 11.0)
        ),
        1,
        "too slow"
    );
    assert_eq!(
        counter.press(
            t + Duration::from_millis(900),
            MouseButton::Left,
            (40.0, 11.0)
        ),
        1,
        "too far"
    );
    assert_eq!(
        counter.press(
            t + Duration::from_millis(1000),
            MouseButton::Right,
            (40.0, 11.0)
        ),
        1,
        "another button"
    );
}

#[test]
fn buttons_are_a_bit_set_like_the_dom() {
    let none = PointerButtons::default();
    let held = none.with(MouseButton::Left).with(MouseButton::Middle);
    assert_eq!(held.0, 5);
    assert!(held.contains(MouseButton::Left) && held.contains(MouseButton::Middle));
    assert!(!held.contains(MouseButton::Right));
    assert_eq!(held.without(MouseButton::Left).0, 4);
    assert_eq!(
        none.with(MouseButton::Other(9)),
        none,
        "a button with no bit sets none"
    );
}

#[test]
fn the_wheel_is_reported_with_the_doms_sign_and_in_notches_or_pixels() {
    let at = |touchpad: f64, scale: f64| WheelScaling {
        step: 64.0,
        touchpad,
        scale,
    };
    // Freya: positive y moves the content down, and a notch is 64 px.
    assert_eq!(
        wheel_delta(WheelSource::Line, 0.0, 64.0, at(1.0, 1.0)),
        WheelDelta::Lines { x: 0.0, y: -1.0 }
    );
    assert_eq!(
        wheel_delta(WheelSource::Line, -128.0, -64.0, at(1.0, 1.0)),
        WheelDelta::Lines { x: 2.0, y: 1.0 }
    );
    assert_eq!(
        wheel_delta(WheelSource::Pixel, 3.0, -12.5, at(1.0, 1.0)),
        WheelDelta::Pixels { x: -3.0, y: 12.5 }
    );
    // A touchpad's pixels are device pixels: on a 2x display 25 of them are
    // 12.5 logical ones, and the window's own touchpad factor is taken off.
    assert_eq!(
        wheel_delta(WheelSource::Pixel, 0.0, -25.0, at(1.0, 2.0)),
        WheelDelta::Pixels { x: 0.0, y: 12.5 }
    );
    assert_eq!(
        wheel_delta(WheelSource::Pixel, 0.0, -25.0, at(2.0, 1.0)),
        WheelDelta::Pixels { x: 0.0, y: 12.5 }
    );
}

#[test]
fn keys_carry_their_dom_names_their_text_and_whether_they_repeat() {
    let mut keys = KeyTracker::default();
    let route = |keys: &mut KeyTracker, key: Key, code: Code, modifiers, pressed| match keys.route(
        &key,
        code,
        modifiers,
        pressed,
        FocusEscape::Tab,
    ) {
        KeyRoute::Key(input) => input,
        _ => panic!("expected a key"),
    };
    let a = Key::Character("a".into());
    let first = route(&mut keys, a.clone(), Code::KeyA, Modifiers::empty(), true);
    assert_eq!(
        (first.key_string().as_str(), first.code_string().as_str()),
        ("a", "KeyA")
    );
    assert_eq!(first.text.as_deref(), Some("a"));
    assert!(!first.repeat);
    let again = route(&mut keys, a.clone(), Code::KeyA, Modifiers::empty(), true);
    assert!(again.repeat, "down again with no up is a repeat");
    let up = route(&mut keys, a.clone(), Code::KeyA, Modifiers::empty(), false);
    assert!(!up.pressed && up.text.is_none());
    assert!(
        !route(&mut keys, a.clone(), Code::KeyA, Modifiers::empty(), true).repeat,
        "after the up it is a first press"
    );

    // Two keys held do not make each other repeat.
    route(&mut keys, a.clone(), Code::KeyA, Modifiers::empty(), false);
    route(&mut keys, a.clone(), Code::KeyA, Modifiers::empty(), true);
    assert!(
        !route(
            &mut keys,
            Key::Character("b".into()),
            Code::KeyB,
            Modifiers::empty(),
            true
        )
        .repeat
    );

    // Ctrl+C types nothing; Enter and arrows type nothing, and have DOM names.
    let ctrl_c = route(
        &mut keys,
        Key::Character("c".into()),
        Code::KeyC,
        Modifiers::CONTROL,
        true,
    );
    assert_eq!(ctrl_c.text, None);
    let enter = route(
        &mut keys,
        Key::Named(NamedKey::Enter),
        Code::Enter,
        Modifiers::empty(),
        true,
    );
    assert_eq!((enter.key_string().as_str(), enter.text), ("Enter", None));
    let left = route(
        &mut keys,
        Key::Named(NamedKey::ArrowLeft),
        Code::ArrowLeft,
        Modifiers::SHIFT,
        true,
    );
    assert_eq!(left.key_string(), "ArrowLeft");

    // Focus leaving forgets held keys: their releases never arrive.
    keys.clear();
    assert!(!route(&mut keys, a, Code::KeyA, Modifiers::empty(), true).repeat);
}

#[test]
fn tab_leaves_or_goes_to_the_producer_and_f6_always_leaves() {
    let mut keys = KeyTracker::default();
    let tab = Key::Named(NamedKey::Tab);
    let f6 = Key::Named(NamedKey::F6);
    let leaves = |route: KeyRoute| matches!(route, KeyRoute::Leave);
    // The default: Tab and Shift+Tab are the window's.
    assert!(leaves(keys.route(
        &tab,
        Code::Tab,
        Modifiers::empty(),
        true,
        FocusEscape::Tab
    )));
    assert!(leaves(keys.route(
        &tab,
        Code::Tab,
        Modifiers::SHIFT,
        true,
        FocusEscape::Tab
    )));
    assert!(leaves(keys.route(
        &tab,
        Code::Tab,
        Modifiers::empty(),
        false,
        FocusEscape::Tab
    )));
    // Ctrl+Tab is not a focus move: the producer may want it (tab switching).
    assert!(!leaves(keys.route(
        &tab,
        Code::Tab,
        Modifiers::CONTROL,
        true,
        FocusEscape::Tab
    )));
    // With `Producer` the producer has Tab, and F6 is still the way out.
    assert!(!leaves(keys.route(
        &tab,
        Code::Tab,
        Modifiers::empty(),
        true,
        FocusEscape::Producer
    )));
    assert!(!leaves(keys.route(
        &tab,
        Code::Tab,
        Modifiers::SHIFT,
        true,
        FocusEscape::Producer
    )));
    assert!(leaves(keys.route(
        &f6,
        Code::F6,
        Modifiers::empty(),
        true,
        FocusEscape::Producer
    )));
    assert!(leaves(keys.route(
        &f6,
        Code::F6,
        Modifiers::SHIFT,
        true,
        FocusEscape::Producer
    )));
}

#[test]
fn text_with_no_physical_key_is_a_commit() {
    let mut keys = KeyTracker::default();
    let committed = keys.route(
        &Key::Character("日本".into()),
        Code::Unidentified,
        Modifiers::empty(),
        true,
        FocusEscape::Tab,
    );
    match committed {
        KeyRoute::Text(TextInput::Commit(text)) => assert_eq!(text, "日本"),
        _ => panic!("expected a commit"),
    }
    // A named key with no code is still a key, and the empty string is nothing.
    let named = keys.route(
        &Key::Named(NamedKey::Enter),
        Code::Unidentified,
        Modifiers::empty(),
        true,
        FocusEscape::Tab,
    );
    assert!(matches!(named, KeyRoute::Key(_)));
    let empty = keys.route(
        &Key::Character(String::new()),
        Code::Unidentified,
        Modifiers::empty(),
        true,
        FocusEscape::Tab,
    );
    assert!(matches!(empty, KeyRoute::Key(_)));
}

#[test]
fn every_cursor_kind_but_hidden_has_an_icon() {
    use super::input::CursorKind::*;
    for kind in [
        Default,
        Pointer,
        Text,
        VerticalText,
        Crosshair,
        Move,
        Grab,
        Grabbing,
        Wait,
        Progress,
        Help,
        ContextMenu,
        Cell,
        Copy,
        Alias,
        NoDrop,
        NotAllowed,
        AllScroll,
        ZoomIn,
        ZoomOut,
        ColResize,
        RowResize,
        EwResize,
        NsResize,
        NeswResize,
        NwseResize,
    ] {
        assert!(kind.icon().is_some(), "{kind:?}");
    }
    assert_eq!(Hidden.icon(), None);
    assert_eq!(Pointer.icon(), Some(CursorIcon::Pointer));
    assert_eq!(super::input::CursorKind::default(), Default);
}

// ---- telling the producer the size ----------------------------------------------

fn size(width: f32, height: f32) -> ViewSize {
    ViewSize {
        width,
        height,
        scale: 1.0,
    }
}

#[test]
fn the_first_size_is_reported_at_once_and_a_burst_makes_one_more_report() {
    let mut reporter = SizeReporter::default();
    let t = Instant::now();
    assert_eq!(
        reporter.offer(size(100.0, 100.0), t),
        Report::Now(size(100.0, 100.0))
    );
    // A drag-resize: a size every 2 ms inside one frame.
    let wait = match reporter.offer(size(110.0, 100.0), t + Duration::from_millis(2)) {
        Report::After(wait) => wait,
        other => panic!("expected a timer, got {other:?}"),
    };
    assert_eq!(wait, REPORT_EVERY - Duration::from_millis(2));
    for step in 2..6 {
        let more = reporter.offer(
            size(100.0 + step as f32 * 10.0, 100.0),
            t + Duration::from_millis(step * 2),
        );
        assert_eq!(more, Report::Nothing, "one timer is enough");
    }
    // The timer ends: the last size is delivered, not the first of the burst.
    assert_eq!(reporter.fire(t + REPORT_EVERY), Some(size(150.0, 100.0)));
    // Nothing more is owed.
    assert_eq!(reporter.fire(t + REPORT_EVERY * 2), None);
}

#[test]
fn a_size_already_told_is_never_told_again() {
    let mut reporter = SizeReporter::default();
    let t = Instant::now();
    reporter.offer(size(100.0, 50.0), t);
    // Layout rounding wanders by hundredths: not a change. This is what keeps a
    // producer that answers with a frame from starting a loop.
    for ms in [20, 40, 60, 500] {
        let same = size(100.004, 49.996);
        assert_eq!(
            reporter.offer(same, t + Duration::from_millis(ms)),
            Report::Nothing
        );
    }
    // After the debounce window a real change goes straight out.
    assert_eq!(
        reporter.offer(size(101.0, 50.0), t + Duration::from_millis(600)),
        Report::Now(size(101.0, 50.0))
    );
    // A change of scale alone is a change.
    let scaled = ViewSize {
        width: 101.0,
        height: 50.0,
        scale: 1.25,
    };
    assert_eq!(
        reporter.offer(scaled, t + Duration::from_millis(700)),
        Report::Now(scaled)
    );
}

#[test]
fn a_size_that_returns_before_the_timer_is_not_reported() {
    let mut reporter = SizeReporter::default();
    let t = Instant::now();
    reporter.offer(size(100.0, 100.0), t);
    assert!(matches!(
        reporter.offer(size(120.0, 100.0), t + Duration::from_millis(3)),
        Report::After(_)
    ));
    reporter.offer(size(100.0, 100.0), t + Duration::from_millis(5));
    assert_eq!(
        reporter.fire(t + REPORT_EVERY),
        None,
        "back where the producer already is"
    );
}

// ---- the wake ---------------------------------------------------------------------

#[test]
fn wakes_coalesce_and_one_before_the_look_still_counts() {
    let signal = WakeSignal::new();
    let waker = signal.waker();
    let mut context = Context::from_waker(Waker::noop());
    let mut notified = std::pin::pin!(signal.notified());
    assert_eq!(notified.as_mut().poll(&mut context), Poll::Pending);
    waker.wake_by_ref();
    waker.wake_by_ref();
    waker.wake_by_ref();
    assert_eq!(
        notified.as_mut().poll(&mut context),
        Poll::Ready(()),
        "three wakes, one look"
    );
    assert_eq!(
        notified.as_mut().poll(&mut context),
        Poll::Pending,
        "and nothing left over"
    );
    // A wake from another thread, before anyone is waiting.
    let from_thread = waker.clone();
    std::thread::spawn(move || from_thread.wake())
        .join()
        .unwrap();
    assert_eq!(notified.as_mut().poll(&mut context), Poll::Ready(()));
    // The first look for a frame made before the view mounted.
    signal.mark();
    assert_eq!(notified.as_mut().poll(&mut context), Poll::Ready(()));
}

// ---- the producer -------------------------------------------------------------------

#[test]
fn the_pattern_is_deterministic_and_carries_its_frame_number() {
    let render = |number, inverted| {
        let mut data = vec![0u8; 200 * 120 * 4];
        pattern::draw(&mut data, 200, 120, number, inverted);
        data
    };
    assert_eq!(render(7, false), render(7, false));
    assert_ne!(render(7, false), render(8, false), "it moves");
    assert_ne!(render(7, false), render(7, true), "and has an up");
    let frame = |number, inverted| {
        let frame = PixelFrame::new(
            200,
            120,
            800,
            PixelFormat::Bgrx8,
            bytes(render(number, inverted)),
        )
        .unwrap();
        frame.with_y_inverted(inverted)
    };
    for number in [0u64, 1, 2, 255, 256, 4660, 65535] {
        for inverted in [false, true] {
            assert_eq!(
                PatternSource::frame_number(&frame(number, inverted)),
                Some(number)
            );
        }
    }
    let tiny = PixelFrame::new(
        PATTERN_MIN_WIDTH - 1,
        50,
        (PATTERN_MIN_WIDTH as usize - 1) * 4,
        PixelFormat::Bgrx8,
        bytes(vec![0; (PATTERN_MIN_WIDTH as usize - 1) * 200]),
    )
    .unwrap();
    assert_eq!(
        PatternSource::frame_number(&tiny),
        None,
        "too small to carry the counter"
    );
}

#[test]
fn the_pattern_draws_at_any_size_without_going_out_of_bounds() {
    for (w, h) in [
        (1, 1),
        (2, 2),
        (3, 50),
        (50, 3),
        (17, 31),
        (130, 40),
        (500, 300),
    ] {
        let mut data = vec![0u8; w * h * 4];
        for number in [0u64, 1, 99] {
            for inverted in [false, true] {
                pattern::draw(&mut data, w as u32, h as u32, number, inverted);
            }
        }
    }
}

#[test]
fn a_producer_resized_gives_frames_of_the_new_size_and_retires_the_old() {
    let source = PatternSource::stepped(100, 60);
    let mut presenter = Presenter::new(source.clone());
    source.step();
    presenter.pump();
    assert_eq!(
        presenter.shown_frame().map(|f| (f.width, f.height)),
        Some((100, 60))
    );
    source.resize(150, 90, 1.5);
    source.step();
    assert_eq!(presenter.pump(), Pump::Shown);
    let frame = presenter.shown_frame().unwrap();
    assert_eq!((frame.width, frame.height, frame.scale), (150, 90, 1.5));
    assert_eq!(frame.logical_size(), (100.0, 60.0));
    // The old-size frame came back and was retired, not reused.
    let stats = source.stats();
    assert_eq!(
        (stats.taken, stats.released, stats.stray_releases),
        (2, 1, 0)
    );
    source.step();
    source.step();
    assert_eq!(presenter.pump(), Pump::Shown);
    assert_eq!(
        presenter.shown_frame().map(|f| (f.width, f.height)),
        Some((150, 90))
    );
    assert_eq!(source.size_hint(), Some((100.0, 60.0)));
}

#[test]
fn the_pattern_reports_a_cursor_and_a_caret_and_wakes_for_them() {
    use super::input::{CaretRect, CursorKind};
    let source = PatternSource::stepped(64, 64);
    let signal = WakeSignal::new();
    source.set_waker(signal.waker());
    let mut context = Context::from_waker(Waker::noop());
    let mut notified = std::pin::pin!(signal.notified());
    assert_eq!(notified.as_mut().poll(&mut context), Poll::Pending);
    assert_eq!(
        (source.cursor(), source.caret()),
        (CursorKind::Default, None)
    );
    source.set_cursor(CursorKind::Text);
    assert_eq!(notified.as_mut().poll(&mut context), Poll::Ready(()));
    let caret = CaretRect {
        x: 4.0,
        y: 5.0,
        width: 1.0,
        height: 16.0,
    };
    source.set_caret(Some(caret));
    assert_eq!(notified.as_mut().poll(&mut context), Poll::Ready(()));
    assert_eq!(
        (source.cursor(), source.caret()),
        (CursorKind::Text, Some(caret))
    );
}

// ---- performance ----------------------------------------------------------------------

mod counting {
    //! Counts the allocations the thread that asks makes. Skia allocates with
    //! its own allocator and is not counted.

    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    pub struct Counting;

    thread_local! {
        static COUNT: Cell<u64> = const { Cell::new(0) };
        static BYTES: Cell<u64> = const { Cell::new(0) };
    }

    // SAFETY: forwards to the system allocator and only counts.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            COUNT.with(|count| count.set(count.get() + 1));
            BYTES.with(|bytes| bytes.set(bytes.get() + layout.size() as u64));
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) }
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            COUNT.with(|count| count.set(count.get() + 1));
            BYTES.with(|bytes| bytes.set(bytes.get() + new_size as u64));
            unsafe { System.realloc(pointer, layout, new_size) }
        }
    }

    /// Allocations and bytes this thread makes while `work` runs.
    pub fn during(work: impl FnOnce()) -> (u64, u64) {
        let (count, bytes) = (COUNT.with(Cell::get), BYTES.with(Cell::get));
        work();
        (COUNT.with(Cell::get) - count, BYTES.with(Cell::get) - bytes)
    }
}

#[global_allocator]
static ALLOCATOR: counting::Counting = counting::Counting;

#[test]
fn showing_and_painting_frames_allocates_nothing_on_the_ui_thread_after_the_first() {
    let source = PatternSource::stepped(320, 180);
    let mut presenter = Presenter::new(source.clone());
    let mut rig = Rig::new(400, 300);
    // The pool fills, and the first paint makes Skia's own state.
    for _ in 0..4 {
        source.step();
        presenter.pump();
        rig.paint(
            &presenter,
            (10.0, 10.0),
            (320.0, 180.0),
            1.0,
            FrameFit::Stretch,
        );
    }
    // The counter does count: a box on the heap is one allocation.
    let (known, _) = counting::during(|| drop(std::hint::black_box(Box::new(7u64))));
    assert_eq!(known, 1);
    let (count, bytes) = counting::during(|| {
        for _ in 0..60 {
            source.step();
            presenter.pump();
            presenter.paint(
                rig.surface.canvas(),
                (320.0, 180.0),
                FrameFit::Stretch,
                &look(),
            );
        }
    });
    assert_eq!((count, bytes), (0, 0), "60 frames taken, shown and painted");
}

/// What drawing a 1920x1080 frame costs, to print. A measurement, not a check
/// (the machine decides the numbers): run it in release mode.
///
/// ```sh
/// cargo test --release --features testing --lib frame_view::tests::measure_a_1080p_frame -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a measurement; see the doc comment for how to run it"]
fn measure_a_1080p_frame() {
    const FRAMES: u32 = 300;
    let each = |total: Duration| total.as_secs_f64() * 1000.0 / f64::from(FRAMES);
    println!("1920x1080, {FRAMES} frames each; the budget at 60 fps is 16.7 ms");
    for (label, format, view, scale) in [
        (
            "opaque (Bgrx8), exact 1:1       ",
            PixelFormat::Bgrx8,
            (1920.0_f32, 1080.0_f32),
            1.0,
        ),
        (
            "premultiplied alpha, exact 1:1  ",
            PixelFormat::Bgra8Premultiplied,
            (1920.0, 1080.0),
            1.0,
        ),
        (
            "opaque, stretched 1920 -> 1600  ",
            PixelFormat::Bgrx8,
            (1600.0, 900.0),
            1.0,
        ),
        (
            "opaque, 1.25x: 1536x864 logical ",
            PixelFormat::Bgrx8,
            (1536.0, 864.0),
            1.25,
        ),
    ] {
        let source = PatternSource::stepped(1920, 1080);
        source.set_format(format);
        let mut presenter = Presenter::new(source.clone());
        let mut rig = Rig::new(1920, 1080);
        let paint = |presenter: &Presenter, rig: &mut Rig| {
            let canvas = rig.surface.canvas();
            canvas.save();
            canvas.scale((scale as f32, scale as f32));
            presenter.paint(canvas, view, FrameFit::Stretch, &look());
            canvas.restore();
        };
        for _ in 0..5 {
            source.step();
            presenter.pump();
            paint(&presenter, &mut rig);
        }
        let (mut pumping, mut painting, mut repainting) =
            (Duration::ZERO, Duration::ZERO, Duration::ZERO);
        let mut allocations = (0, 0);
        for _ in 0..FRAMES {
            source.step();
            let (count, bytes) = counting::during(|| {
                let started = Instant::now();
                presenter.pump();
                pumping += started.elapsed();
                let started = Instant::now();
                paint(&presenter, &mut rig);
                painting += started.elapsed();
            });
            allocations = (allocations.0 + count, allocations.1 + bytes);
            // The same frame again: a repaint that brings nothing new.
            let started = Instant::now();
            paint(&presenter, &mut rig);
            repainting += started.elapsed();
        }
        println!(
            "{label} take+show {:.3} ms, paint a new frame {:.2} ms, repaint the same {:.2} ms; Rust allocations per frame: {}",
            each(pumping),
            each(painting),
            each(repainting),
            allocations.0 / u64::from(FRAMES),
        );
    }
    // One pass over the pixels, for scale: copying the frame to another buffer.
    let source = PatternSource::stepped(1920, 1080);
    source.step();
    let mut presenter = Presenter::new(source);
    presenter.pump();
    let frame = presenter.shown_frame().unwrap();
    let mut copy = vec![0u8; frame.pixels().len()];
    let started = Instant::now();
    for _ in 0..FRAMES {
        copy.copy_from_slice(std::hint::black_box(frame.pixels()));
        std::hint::black_box(&copy);
    }
    println!(
        "for scale: memcpy of the frame (8.3 MB) {:.2} ms",
        each(started.elapsed())
    );
}
