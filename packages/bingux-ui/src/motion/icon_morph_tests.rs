//! The morphing icon button, frame by frame: a headless window steps the
//! animation clock 16 ms at a time, and the tests read the pixels of the
//! button's icon slot, the button's box and its accessible name.
//!
//! The button is `icon_button(Refresh).morph(MediaPlaybackStop, on)`: the
//! Files toolbar's Reload and Stop. Pressing `x` flips `on`.

use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use torin::prelude::Area;

use super::*;
use crate::controls::icon_button;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::{Scheme, SharedTokens, Tokens};

/// The wall time of a frame, longer than the clock's 34 ms tick cap.
const STEP: Duration = Duration::from_millis(40);
/// The clock's speed that makes a capped 34 ms tick 16.5 ms, cut to 16: every
/// frame moves every animation by exactly 16 ms.
const SPEED: f32 = 0.4853;
const SIZE: (f32, f32) = (120.0, 80.0);
/// The frames a morph of [`MOVE_MS`] needs, and a few more.
const FRAMES: usize = (MOVE_MS as usize).div_ceil(16) + 4;

#[derive(Clone, PartialEq)]
struct Scene {
    reduced: bool,
    start_on: bool,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let reduced = self.reduced;
        let shared = use_hook(move || SharedTokens(State::create(Tokens::new("blue", Scheme::PreferDark, reduced))));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let start = self.start_on;
        let mut on = use_state(move || start);
        rect()
            .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg), p(S4)])
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                if event.key == Key::Character("x".into()) {
                    let next = !*on.peek();
                    on.set(next);
                }
            })
            .child(
                icon_button(Icon::Refresh, |_| {})
                    .morph(Icon::MediaPlaybackStop, on())
                    .tooltip(if on() { "Stop (Esc)" } else { "Refresh (F5)" }),
            )
    }
}

fn shot(scene: Scene) -> Snapshot {
    let mut shot = Snapshot::new(move || scene.clone().into(), SIZE);
    shot.settle(Duration::from_millis(300));
    shot.runner().animation_clock().set_speed(SPEED);
    shot
}

/// One frame: the same 16 ms of animation every time.
fn frame(shot: &mut Snapshot) {
    shot.runner().poll_n(STEP, 1);
}

fn flip(shot: &mut Snapshot) {
    shot.key(Key::Character("x".into()));
}

/// The button's box and name now.
fn button(shot: &mut Snapshot) -> (Area, String) {
    let found = shot.icon_buttons();
    assert_eq!(found.len(), 1, "one icon button");
    (found[0].area, found[0].name.clone())
}

/// The pixels of a window's area, as the renderer drew it.
#[derive(Clone)]
struct Crop {
    rgb: Vec<[u8; 3]>,
}

impl Crop {
    fn of(shot: &mut Snapshot, area: Area) -> Crop {
        let png = shot.runner().render();
        let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
        let mut surface = raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
        surface.canvas().draw_image(&image, (0, 0), None);
        let info = surface.canvas().image_info();
        let row = info.min_row_bytes();
        let mut bytes = vec![0u8; info.compute_byte_size(row)];
        assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
        let mut rgb = Vec::new();
        for y in area.min_y().floor() as usize..area.max_y().ceil() as usize {
            for x in area.min_x().floor() as usize..area.max_x().ceil() as usize {
                let at = y * row + x * 4;
                // N32 on a little-endian machine is blue, green, red, alpha.
                rgb.push([bytes[at + 2], bytes[at + 1], bytes[at]]);
            }
        }
        Crop { rgb }
    }

    /// The summed difference to `other`, in levels.
    fn distance(&self, other: &Crop) -> f64 {
        self.rgb
            .iter()
            .zip(&other.rgb)
            .map(|(a, b)| (0..3).map(|c| f64::from(a[c].abs_diff(b[c]))).sum::<f64>())
            .sum()
    }
}

/// `frames` frames after a flip: the pixels of the button each time.
fn strip(shot: &mut Snapshot, area: Area, frames: usize) -> Vec<Crop> {
    (0..frames)
        .map(|_| {
            frame(shot);
            Crop::of(shot, area)
        })
        .collect()
}

#[test]
fn the_name_and_box_flip_with_the_state_and_the_box_never_moves() {
    let mut shot = shot(Scene { reduced: false, start_on: false });
    let (rest, name) = button(&mut shot);
    assert_eq!(name, "Refresh (F5)");
    flip(&mut shot);
    for _ in 0..FRAMES {
        frame(&mut shot);
        let (area, name) = button(&mut shot);
        assert_eq!(area, rest, "the button keeps its box through the morph");
        assert_eq!(name, "Stop (Esc)", "the name changes with the state, not with the motion");
    }
    flip(&mut shot);
    for _ in 0..FRAMES {
        frame(&mut shot);
        let (area, name) = button(&mut shot);
        assert_eq!(area, rest);
        assert_eq!(name, "Refresh (F5)");
    }
}

#[test]
fn the_icon_moves_from_reload_to_stop_one_frame_at_a_time() {
    let mut shot = shot(Scene { reduced: false, start_on: false });
    let (area, _) = button(&mut shot);
    let reload = Crop::of(&mut shot, area);
    flip(&mut shot);
    let frames = strip(&mut shot, area, FRAMES);
    let stop = frames.last().unwrap().clone();
    let total = reload.distance(&stop);
    assert!(total > 1500.0, "reload and stop differ: {total}");
    // The way out of reload only grows, the way into stop only shrinks.
    let slack = total * 0.03;
    let from_reload: Vec<f64> = frames.iter().map(|crop| crop.distance(&reload)).collect();
    let to_stop: Vec<f64> = frames.iter().map(|crop| crop.distance(&stop)).collect();
    assert!(from_reload.windows(2).all(|pair| pair[1] >= pair[0] - slack), "away from reload: {from_reload:?}");
    assert!(to_stop.windows(2).all(|pair| pair[1] <= pair[0] + slack), "towards stop: {to_stop:?}");
    // It really is a motion: some frame is neither glyph, and it does not
    // arrive in one frame.
    assert!(
        frames.iter().any(|crop| crop.distance(&reload) > total * 0.2 && crop.distance(&stop) > total * 0.2),
        "a frame in between: {from_reload:?}"
    );
    assert!(frames[0].distance(&stop) > total * 0.3, "the first frame is not there yet");
    // Settled: the same picture two frames later.
    frame(&mut shot);
    assert!(Crop::of(&mut shot, area).distance(&stop) < total * 0.01);
    // And back.
    flip(&mut shot);
    let back = strip(&mut shot, area, FRAMES);
    let to_reload: Vec<f64> = back.iter().map(|crop| crop.distance(&reload)).collect();
    assert!(to_reload.windows(2).all(|pair| pair[1] <= pair[0] + slack), "towards reload: {to_reload:?}");
    assert!(*to_reload.last().unwrap() < total * 0.01, "back to the reload glyph: {to_reload:?}");
}

#[test]
fn a_flip_half_way_goes_back_from_where_the_icon_is() {
    let mut shot = shot(Scene { reduced: false, start_on: false });
    let (area, _) = button(&mut shot);
    let reload = Crop::of(&mut shot, area);
    // Both ends, from one full trip there and back.
    flip(&mut shot);
    let stop = strip(&mut shot, area, FRAMES).pop().unwrap();
    flip(&mut shot);
    strip(&mut shot, area, FRAMES);
    let total = reload.distance(&stop);

    flip(&mut shot);
    let out = strip(&mut shot, area, 5);
    let turned_at = out.last().unwrap().distance(&reload);
    assert!(turned_at > total * 0.2 && turned_at < total * 0.95, "half way: {turned_at} of {total}");
    flip(&mut shot);
    let back = strip(&mut shot, area, FRAMES);
    // The first frame back continues from where it was: the picture moves a
    // frame's worth, it does not jump to either end.
    let step = back[0].distance(out.last().unwrap());
    assert!(step < total * 0.45, "continuous at the turn: {step} of {total}");
    assert!(back[0].distance(&reload) > total * 0.02, "no snap to the start");
    let to_reload: Vec<f64> = back.iter().map(|crop| crop.distance(&reload)).collect();
    let slack = total * 0.03;
    assert!(to_reload.windows(2).all(|pair| pair[1] <= pair[0] + slack), "and on to reload: {to_reload:?}");
    assert!(*to_reload.last().unwrap() < total * 0.01);
}

#[test]
fn reduced_motion_swaps_at_once() {
    let mut shot = shot(Scene { reduced: true, start_on: false });
    let (area, _) = button(&mut shot);
    let reload = Crop::of(&mut shot, area);
    flip(&mut shot);
    let frames = strip(&mut shot, area, 3);
    let total = reload.distance(frames.last().unwrap());
    assert!(total > 1500.0);
    for crop in &frames {
        assert!(crop.distance(frames.last().unwrap()) < total * 0.01, "every frame after the flip is the stop glyph");
    }
}

#[test]
fn the_hover_fill_and_the_box_hold_through_the_morph() {
    let mut shot = shot(Scene { reduced: false, start_on: false });
    let (area, _) = button(&mut shot);
    let (x, y) = (area.min_x() + area.width() / 2.0, area.min_y() + area.height() / 2.0);
    shot.hover(x, y);
    frame(&mut shot);
    let before = shot.icon_buttons()[0].fill;
    assert!(before.is_some(), "a hovered button has its fill");
    flip(&mut shot);
    for _ in 0..FRAMES {
        frame(&mut shot);
        let look = &shot.icon_buttons()[0];
        assert_eq!(look.area, area);
        assert_eq!(look.fill, before, "hover state is the button's, not the glyph's");
    }
}

#[test]
fn a_button_that_mounts_on_does_not_animate() {
    let mut shot = shot(Scene { reduced: false, start_on: true });
    let (area, name) = button(&mut shot);
    assert_eq!(name, "Stop (Esc)");
    let first = Crop::of(&mut shot, area);
    frame(&mut shot);
    assert!(Crop::of(&mut shot, area).distance(&first) < 50.0, "already at the stop glyph");
}

/// Mount the scene at 6x, and save `frames` frames of 16 ms; the button is
/// flipped before the frames whose indexes are in `flips` (1 is the first).
/// With `hover` the pointer rests on the button first.
fn record(name: &str, flips: &[usize], frames: usize, hover: bool) {
    let Some(dir) = std::env::var_os("BINGUX_FRAME_DIR").map(std::path::PathBuf::from) else {
        return;
    };
    let mut shot = Snapshot::with_scale(
        || Scene { reduced: false, start_on: false }.into(),
        SIZE,
        6.0,
    );
    shot.settle(Duration::from_millis(300));
    shot.runner().animation_clock().set_speed(SPEED);
    if hover {
        // The runner's pointer units are physical pixels at this scale.
        let (area, _) = button(&mut shot);
        shot.hover(area.min_x() + area.width() / 2.0, area.min_y() + area.height() / 2.0);
        frame(&mut shot);
    }
    shot.save(dir.join(format!("{name}-00.png")));
    for index in 1..=frames {
        if flips.contains(&index) {
            flip(&mut shot);
        }
        frame(&mut shot);
        shot.save(dir.join(format!("{name}-{index:02}.png")));
    }
}

#[test]
#[ignore = "writes frame strips when BINGUX_FRAME_DIR is set"]
fn frame_strips() {
    record("to-stop", &[1], 14, false);
    record("round-trip", &[1, 15], 28, false);
    record("turned-half-way", &[1, 6], 20, false);
    record("rapid", &[1, 3, 5, 7, 9], 22, false);
    record("hovered", &[1], 14, true);
}
