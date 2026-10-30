//! Indeterminate progress keeps moving.
//!
//! The indeterminate bar once sat still in every window: its segment was
//! shifted with `offset_x`, which moves a box's *children*, and the segment
//! has none. A test that only looked at the animation's value passed, because
//! the value did change; these read where the segment is laid out and what the
//! renderer draws, one animation tick at a time.
//!
//! The animation clock runs at its top speed and every step waits longer than
//! the animation's 34 ms tick cap, so each step moves a loop by the same
//! 170 ms whatever the machine's load. A sweep lasts 1400 ms, a little over
//! eight steps, so twenty steps cover more than two sweeps.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;
use torin::prelude::Area;

use super::progress::{SpinnerSize, progress_bar, spinner};
use crate::component::ComponentName;
use crate::motion::use_cycle;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::{DEFAULT_ACCENT, Scheme, SharedTokens, Tokens, freya_theme};

const SIZE: (f32, f32) = (480.0, 200.0);
/// The animation clock's top speed: one step is 170 ms of a loop.
const SPEED: f32 = 5.0;
/// Its bottom speed, for mounting a loop and looking at it before it has moved.
const CRAWL: f32 = 0.05;
/// A sweep, in steps of 170 ms, give or take the rounding of each step.
const SWEEP_STEPS: std::ops::RangeInclusive<usize> = 7..=10;

#[derive(Clone, Copy, PartialEq)]
enum Scene {
    /// An indeterminate bar with a caption that changes when the page draws again.
    Bar,
    /// A determinate bar.
    Done,
    Spinner(SpinnerSize),
}

/// The window root: the tokens of a dark window, with reduced motion or not.
#[derive(PartialEq)]
struct Root {
    scene: Scene,
    reduced: bool,
}

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let tokens = Tokens::new(DEFAULT_ACCENT, Scheme::PreferDark, self.reduced);
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        Page(self.scene)
    }
}

/// The scene, and a box that draws the page again when it is pressed.
#[derive(PartialEq)]
struct Page(Scene);

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut draws = use_state(|| 0_u32);
        let shown: Element = match self.0 {
            Scene::Bar => progress_bar(None)
                .text(format!("Copying {} of 10", draws.read()))
                .into(),
            Scene::Done => progress_bar(0.5).into(),
            Scene::Spinner(size) => spinner(size).into(),
        };
        rect()
            .expanded()
            .tw(&tokens, &[flex_col(), gap(S4), p(S4)])
            .child(shown)
            .child(
                rect()
                    .component("Redraw")
                    .tw(&tokens, &[w(Dim::Field), h(Dim::Control)])
                    .on_pointer_down(move |_: Event<PointerEventData>| *draws.write() += 1),
            )
    }
}

fn open(scene: Scene, reduced: bool) -> Snapshot {
    let mut shot = Snapshot::new(move || Root { scene, reduced }.into_element(), SIZE);
    // Mounted at the slowest clock: the loop is still where it began when
    // the first look at it is taken.
    shot.runner().animation_clock().set_speed(CRAWL);
    for _ in 0..4 {
        step(&mut shot);
    }
    shot.runner().animation_clock().set_speed(SPEED);
    shot
}

/// One step: wait past the tick cap, so every loop moves by the same amount.
fn step(shot: &mut Snapshot) {
    shot.runner().poll_n(Duration::from_millis(40), 1);
}

fn area_of(shot: &mut Snapshot, class: &'static str) -> Option<Area> {
    shot.runner()
        .find_many(|node, element| {
            (element.accessibility().builder.class_name() == Some(class)).then(|| node.layout().area)
        })
        .into_iter()
        .next()
}

/// Whether a label or paragraph on screen reads `text`.
fn shows(shot: &mut Snapshot, text: &str) -> bool {
    !shot
        .runner()
        .find_many(|_, element| {
            let shown: String = Label::try_downcast(element).map(|label| label.text.to_string()).or_else(|| {
                Paragraph::try_downcast(element)
                    .map(|paragraph| paragraph.spans.iter().map(|span| span.text.to_string()).collect())
            })?;
            (shown == text).then_some(())
        })
        .is_empty()
}

/// Where the sweeping segment's left edge is, in percent of the track's width
/// from the track's left edge. `None` when there is no segment.
fn left(shot: &mut Snapshot) -> Option<f32> {
    let track = area_of(shot, "ProgressTrack").expect("the bar has a track");
    let segment = area_of(shot, "ProgressSegment")?;
    Some((segment.min_x() - track.min_x()) / track.width() * 100.0)
}

/// `steps` positions of the sweeping segment, one after each step.
fn sweep(shot: &mut Snapshot, steps: usize) -> Vec<f32> {
    (0..steps)
        .map(|_| {
            step(shot);
            left(shot).expect("the sweeping segment is on screen")
        })
        .collect()
}

/// The index of the first step on which the segment went back to the start.
fn first_wrap(positions: &[f32]) -> Option<usize> {
    (1..positions.len()).find(|&at| positions[at] < positions[at - 1] - 50.0)
}

/// One frame as the renderer drew it.
struct Pixels {
    bytes: Vec<u8>,
    row: usize,
    width: usize,
    height: usize,
}

impl Pixels {
    fn of(shot: &mut Snapshot) -> Self {
        let png = shot.runner().render();
        let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
        let mut surface = raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
        surface.canvas().draw_image(&image, (0, 0), None);
        let info = surface.canvas().image_info();
        let row = info.min_row_bytes();
        let mut bytes = vec![0u8; info.compute_byte_size(row)];
        assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
        Self { bytes, row, width: image.width() as usize, height: image.height() as usize }
    }

    /// The pixel at a window point, as red, green and blue.
    fn at(&self, x: f32, y: f32) -> [u8; 3] {
        let x = (x.round().max(0.0) as usize).min(self.width - 1);
        let y = (y.round().max(0.0) as usize).min(self.height - 1);
        let at = y * self.row + x * 4;
        // N32 on a little-endian machine is blue, green, red, alpha.
        [self.bytes[at + 2], self.bytes[at + 1], self.bytes[at]]
    }

    /// Every pixel of `area`, to tell one frame of it from another.
    fn region(&self, area: Area) -> Vec<u8> {
        let (x0, x1) = (area.min_x().floor().max(0.0) as usize, (area.max_x().ceil() as usize).min(self.width));
        let (y0, y1) = (area.min_y().floor().max(0.0) as usize, (area.max_y().ceil() as usize).min(self.height));
        let mut out = Vec::new();
        for y in y0..y1 {
            out.extend_from_slice(&self.bytes[y * self.row + x0 * 4..y * self.row + x1 * 4]);
        }
        out
    }
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 3)
}

fn rgb(colour: Color) -> [u8; 3] {
    [colour.r(), colour.g(), colour.b()]
}

fn dark(reduced: bool) -> Tokens {
    Tokens::new(DEFAULT_ACCENT, Scheme::PreferDark, reduced)
}

#[test]
fn the_segment_starts_off_the_left_edge() {
    let mut shot = open(Scene::Bar, false);
    let track = area_of(&mut shot, "ProgressTrack").expect("the bar has a track");
    let segment = area_of(&mut shot, "ProgressSegment").expect("the segment is on screen once the track is measured");
    assert!(
        segment.max_x() <= track.min_x() + 1.0,
        "a sweep begins with the segment entirely left of the track: the track is {track:?} and the segment {segment:?}"
    );
    assert!(
        (segment.width() / track.width() * 100.0 - 30.0).abs() < 1.0,
        "the segment is 30% of the track: {segment:?} in {track:?}"
    );
}

#[test]
fn the_segment_moves_on_every_step() {
    let mut shot = open(Scene::Bar, false);
    let positions = sweep(&mut shot, 20);
    println!("sweep, % of the track: {positions:.1?}");
    let wrap = first_wrap(&positions).expect("a sweep ends within twenty steps");
    assert!(
        SWEEP_STEPS.contains(&wrap),
        "a sweep of 1400 ms is {SWEEP_STEPS:?} steps of 170 ms, this one was {wrap}: {positions:.1?}"
    );
    // The last step of a sweep only lands on the end, a hair past the one before.
    for at in 1..wrap - 1 {
        assert!(
            positions[at] > positions[at - 1] + 0.5,
            "the segment did not move on step {at}: {positions:.1?}"
        );
    }
    assert!(
        positions[..wrap].iter().any(|&at| at > 40.0 && at < 60.0),
        "the segment crosses the middle of the track: {positions:.1?}"
    );
}

#[test]
fn a_sweep_wraps_to_the_start_and_goes_again() {
    let mut shot = open(Scene::Bar, false);
    let positions = sweep(&mut shot, 20);
    let wrap = first_wrap(&positions).expect("a sweep ends within twenty steps");
    assert!(
        positions[wrap - 1] > 90.0,
        "a sweep ends with the segment off the right edge: {positions:.1?}"
    );
    assert!(
        (positions[wrap] + 30.0).abs() < 1.5,
        "the next sweep begins off the left edge, at -30%: {positions:.1?}"
    );
    assert!(
        positions[wrap..wrap + 4].windows(2).all(|pair| pair[1] > pair[0] + 0.5),
        "the second sweep moves on as the first did: {positions:.1?}"
    );
}

#[test]
fn the_sweep_is_drawn_inside_the_track() {
    let mut shot = open(Scene::Bar, false);
    let accent = rgb(dark(false).accent);
    // Walk to a step with the segment well inside the track.
    let mut seen = false;
    for _ in 0..12 {
        step(&mut shot);
        let track = area_of(&mut shot, "ProgressTrack").expect("the bar has a track");
        let segment = area_of(&mut shot, "ProgressSegment").expect("a segment");
        if segment.min_x() < track.min_x() + 10.0 || segment.max_x() > track.max_x() - 10.0 {
            continue;
        }
        let frame = Pixels::of(&mut shot);
        let y = track.min_y() + track.height() / 2.0;
        assert!(
            near(frame.at(segment.min_x() + segment.width() / 2.0, y), accent),
            "the segment is drawn in the accent where it is laid out: {:?}",
            frame.at(segment.min_x() + segment.width() / 2.0, y)
        );
        assert!(
            !near(frame.at(segment.min_x() - 6.0, y), accent) && !near(frame.at(segment.max_x() + 6.0, y), accent),
            "the track beside the segment is not accent"
        );
        seen = true;
        break;
    }
    assert!(seen, "the segment was inside the track at some step");
}

#[test]
fn a_clipped_segment_stays_inside_the_track() {
    let mut shot = open(Scene::Bar, false);
    let accent = rgb(dark(false).accent);
    // The step on which the segment is half way out of the right edge.
    for _ in 0..20 {
        step(&mut shot);
        let track = area_of(&mut shot, "ProgressTrack").expect("the bar has a track");
        let segment = area_of(&mut shot, "ProgressSegment").expect("a segment");
        if segment.max_x() > track.max_x() + 10.0 && segment.min_x() < track.max_x() - 10.0 {
            let frame = Pixels::of(&mut shot);
            let y = track.min_y() + track.height() / 2.0;
            assert!(near(frame.at(track.max_x() - 8.0, y), accent), "the part inside the track is drawn");
            assert!(
                !near(frame.at(track.max_x() + 4.0, y), accent),
                "the part past the track's edge is clipped"
            );
            return;
        }
    }
    panic!("the segment never straddled the right edge");
}

#[test]
fn the_sweep_goes_on_when_the_page_draws_again() {
    let mut shot = open(Scene::Bar, false);
    // Three steps in, the segment is part-way across.
    let before = *sweep(&mut shot, 3).last().expect("three steps");
    assert!(before > -20.0 && before < 40.0, "three steps in, the segment is on its way: {before}");
    let redraw = area_of(&mut shot, "Redraw").expect("the page has a box to press");
    shot.click(redraw.min_x() + redraw.width() / 2.0, redraw.min_y() + redraw.height() / 2.0);
    assert!(
        shows(&mut shot, "Copying 1 of 10"),
        "pressing the box drew the page and the bar's caption again"
    );
    // Pressing a box lets one animation tick through before the page draws again.
    let after = left(&mut shot).expect("the segment is still on screen after the page drew again");
    assert!(
        after > before - 1.0,
        "drawing the page again did not start the sweep over ({before} then {after})"
    );
    // The first step after the press has no tick left to run, so it stands where it was.
    let next = sweep(&mut shot, 3);
    assert!(
        next.iter().all(|&at| at > after - 1.0) && next[2] > after + 20.0,
        "the sweep went on from where it was ({after}) and did not start again from -30%: {next:.1?}"
    );
}

#[test]
fn a_finished_bar_does_not_loop() {
    let mut shot = open(Scene::Done, false);
    for _ in 0..3 {
        step(&mut shot);
    }
    assert!(
        area_of(&mut shot, "ProgressSegment").is_none(),
        "a bar that knows how far it is has no sweeping segment"
    );
}

/// What the bar draws under reduced motion: no travel, a breath.
#[test]
fn reduced_motion_breathes_instead_of_standing_still() {
    let mut shot = open(Scene::Bar, true);
    let tokens = dark(true);
    let ground = rgb(Paint::Outline.color(&tokens));
    let mut seen = Vec::new();
    for _ in 0..16 {
        step(&mut shot);
        let track = area_of(&mut shot, "ProgressTrack").expect("the bar has a track");
        let fill = area_of(&mut shot, "ProgressSegment").expect("the breathing fill is on screen");
        assert!(
            (fill.min_x() - track.min_x()).abs() < 1.0 && (fill.width() - track.width()).abs() < 1.0,
            "under reduced motion nothing travels: the fill is the whole track, {fill:?} in {track:?}"
        );
        let frame = Pixels::of(&mut shot);
        let pixel = frame.at(track.min_x() + track.width() / 2.0, track.min_y() + track.height() / 2.0);
        assert!(!near(pixel, ground), "the bar is never invisible: {pixel:?} on {ground:?}");
        seen.push(pixel);
    }
    let mut distinct = seen.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(distinct.len() >= 8, "the fill changes on nearly every step: {} distinct of {seen:?}", distinct.len());
    let swing = (0..3)
        .map(|channel| {
            let values = seen.iter().map(|pixel| pixel[channel]);
            values.clone().max().unwrap_or(0) - values.min().unwrap_or(0)
        })
        .max()
        .unwrap_or(0);
    assert!(swing >= 40, "the breath is a visible change, {swing} levels at most: {seen:?}");
}

/// A spinner's frames over `steps` steps: its pixels after each.
fn spinner_frames(reduced: bool, steps: usize) -> Vec<Vec<u8>> {
    let mut shot = open(Scene::Spinner(SpinnerSize::Large), reduced);
    (0..steps)
        .map(|_| {
            step(&mut shot);
            let area = area_of(&mut shot, "Spinner").expect("the spinner is on screen");
            Pixels::of(&mut shot).region(area)
        })
        .collect()
}

/// Frames that a loop keeps changing: it never draws the same frame three
/// times running. (A step is 170 ms, so the step that lands on the end of a
/// turn draws the angle the next one starts from: two equal frames in a row
/// at each turn, and no more.)
fn keeps_turning(frames: &[Vec<u8>], what: &str) {
    for (at, run) in frames.windows(3).enumerate() {
        assert!(
            run[0] != run[1] || run[1] != run[2],
            "{what} stood still from step {at} to step {}",
            at + 2
        );
    }
    let mut distinct: Vec<&Vec<u8>> = frames.iter().collect();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(distinct.len() >= 5, "{what} drew {} different frames in {} steps", distinct.len(), frames.len());
}

#[test]
fn the_spinner_turns_for_longer_than_a_turn() {
    // 800 ms a turn: sixteen steps are more than three turns.
    keeps_turning(&spinner_frames(false, 16), "the spinner");
}

#[test]
fn a_spinner_under_reduced_motion_still_turns() {
    // 2400 ms a turn: eighteen steps are more than one turn.
    keeps_turning(&spinner_frames(true, 18), "the slow spinner");
}

/// The loop clock under every indeterminate indicator.
#[derive(Clone)]
struct Clock(Rc<Cell<f32>>);

impl PartialEq for Clock {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Component for Clock {
    fn render(&self) -> impl IntoElement {
        self.0.set(use_cycle(800));
        rect()
    }
}

#[test]
fn the_cycle_runs_from_nothing_to_one_and_starts_over() {
    let phase = Rc::new(Cell::new(f32::NAN));
    let seen = phase.clone();
    let mut shot = Snapshot::new(move || Clock(seen.clone()).into_element(), SIZE);
    shot.runner().animation_clock().set_speed(SPEED);
    let mut phases = Vec::new();
    for _ in 0..16 {
        step(&mut shot);
        phases.push(phase.get());
    }
    println!("phase after each step: {phases:.3?}");
    assert!(phases.iter().all(|p| (0.0..=1.0).contains(p)), "a phase stays within 0..=1: {phases:?}");
    // The first tick of a loop counts for nothing, so it stands still for two steps.
    let moving = &phases[2..];
    let wraps = moving.windows(2).filter(|pair| pair[1] < pair[0]).count();
    assert!(wraps >= 2, "sixteen steps of 170 ms are more than two cycles of 800 ms: {phases:?}");
    for pair in moving.windows(2) {
        let rises = pair[1] > pair[0];
        let starts_over = pair[0] > 0.99 && pair[1] < 0.01;
        assert!(rises || starts_over, "every step moves on, or starts the next cycle from 0: {phases:?}");
    }
}
