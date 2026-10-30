//! The number component, frame by frame: a headless window steps one 16 ms
//! frame at a time and the tests read where each digit of each column is, how
//! opaque it is and where the neighbours of the number sit.
//!
//! With `BINGUX_FRAME_DIR=DIR` the ignored `frame_strips` test also writes
//! the frames as PNGs, a prefix per scene.

use std::path::PathBuf;
use std::time::Duration;

use accesskit::{Live, Role};
use freya_core::element::ElementExt;

use super::*;
use crate::snapshot::Snapshot;
use crate::theme::{Scheme, SharedTokens, Tokens};
use crate::typography::{CAPTION, HERO};

/// The wall time of a frame, longer than the clock's 34 ms tick cap.
const STEP: Duration = Duration::from_millis(40);
/// The clock's speed that makes a capped 34 ms tick 16.5 ms, cut to 16: every
/// frame moves every animation by exactly 16 ms.
const SPEED: f32 = 0.4853;
const SIZE: (f32, f32) = (420.0, 90.0);
/// The frames a roll of [`ROLL_MS`] needs, and a couple more.
const ROLL_FRAMES: usize = (ROLL_MS as usize).div_ceil(16) + 3;

#[derive(Clone, PartialEq)]
struct Scene {
    values: Vec<f64>,
    decimals: usize,
    min_digits: usize,
    grouped: bool,
    style: TextStyle,
    prefix: Option<&'static str>,
    suffix: Option<&'static str>,
    reduced: bool,
    silent: bool,
}

impl Scene {
    fn new(values: &[f64]) -> Self {
        Self {
            values: values.to_vec(),
            decimals: 0,
            min_digits: 1,
            grouped: false,
            style: NUMERIC,
            prefix: None,
            suffix: None,
            reduced: false,
            silent: false,
        }
    }
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let reduced = self.reduced;
        let shared = use_hook(move || SharedTokens(State::create(Tokens::new("blue", Scheme::PreferDark, reduced))));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let mut index = use_state(|| 0_usize);
        let last = self.values.len() - 1;
        let value = self.values[index().min(last)];
        let mut number = animated_number(value)
            .decimals(self.decimals)
            .min_digits(self.min_digits)
            .style(self.style);
        if self.grouped {
            number = number.grouped();
        }
        if let Some(prefix) = self.prefix {
            number = number.prefix(prefix);
        }
        if let Some(suffix) = self.suffix {
            number = number.suffix(suffix);
        }
        if self.silent {
            number = number.silent();
        }
        rect()
            .expanded()
            .tw(&tokens, &[flex_col(), p(S4), bg(Paint::Bg)])
            .on_press(move |_| index.set((index() + 1).min(last)))
            .child(
                rect()
                    .horizontal()
                    .child(styled_label(self.style, "Before"))
                    .child(number)
                    .child(styled_label(self.style, "After")),
            )
    }
}

fn shot(scene: Scene) -> Snapshot {
    shot_sized(scene, SIZE)
}

fn shot_sized(scene: Scene, size: (f32, f32)) -> Snapshot {
    let mut shot = Snapshot::new(move || scene.clone().into(), size);
    shot.settle(Duration::from_millis(200));
    shot.runner().animation_clock().set_speed(SPEED);
    shot
}

/// One frame: a wait past the tick cap, so the animation clock moves by the
/// same 16 ms every time, whatever else the machine is doing.
fn frame(shot: &mut Snapshot) {
    shot.runner().poll_n(STEP, 1);
}

/// Enough frames for every roll to have finished.
fn frames_at_rest(shot: &mut Snapshot) {
    for _ in 0..ROLL_FRAMES * 2 {
        frame(shot);
    }
}

/// Moves the scene on to its next value.
fn tap(shot: &mut Snapshot) {
    shot.click(2.0, 2.0);
}

/// One glyph on show in a column: its box relative to the top of the column,
/// and how opaque it is.
#[derive(Clone, Debug)]
struct Seen {
    ch: char,
    y: f32,
    width: f32,
    top: f32,
    opacity: f32,
}

/// One clipped column of the number.
#[derive(Clone, Debug)]
struct Column {
    x: f32,
    width: f32,
    top: f32,
    height: f32,
    glyphs: Vec<Seen>,
}

impl Column {
    fn glyph(&self, ch: char) -> Option<&Seen> {
        self.glyphs.iter().find(|seen| seen.ch == ch)
    }

    /// The glyph that is at rest, if one is.
    fn resting(&self) -> Option<&Seen> {
        self.glyphs.iter().find(|seen| seen.y.abs() < 0.01 && seen.opacity > 0.999)
    }
}

fn text_of(element: &dyn ElementExt) -> Option<String> {
    Label::try_downcast(element).map(|label| label.text.to_string())
}

/// Every clipped column that holds a glyph, left to right.
fn columns(shot: &mut Snapshot) -> Vec<Column> {
    let mut found: Vec<Column> = shot.runner().find_many(|node, element| {
        let effect = element.effect()?;
        if effect.overflow != Overflow::Clip {
            return None;
        }
        let area = node.layout().area;
        let glyphs: Vec<Seen> = node
            .children()
            .into_iter()
            .filter_map(|holder| {
                let opacity = holder.element().effect().and_then(|effect| effect.opacity).unwrap_or(1.0);
                let (ch, label) = holder.children().into_iter().find_map(|child| {
                    let text = text_of(child.element().as_ref())?;
                    let mut chars = text.chars();
                    let ch = chars.next()?;
                    chars.next().is_none().then(|| (ch, child.layout().area))
                })?;
                Some(Seen {
                    ch,
                    y: label.min_y() - area.min_y(),
                    width: label.width(),
                    top: label.min_y(),
                    opacity,
                })
            })
            .collect();
        (!glyphs.is_empty()).then(|| Column {
            x: area.min_x(),
            width: area.width(),
            top: area.min_y(),
            height: area.height(),
            glyphs,
        })
    });
    found.sort_by(|a, b| a.x.total_cmp(&b.x));
    found
}

/// The box of the label that says `text`.
fn label_area(shot: &mut Snapshot, text: &str) -> Area {
    shot.runner()
        .find_many(|node, element| (text_of(element)? == text).then(|| node.layout().area))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no label {text:?}"))
}

/// The text the columns show at rest.
fn rest_text(columns: &[Column]) -> String {
    columns.iter().filter_map(|column| column.resting().map(|seen| seen.ch)).collect()
}

/// What one frame looks like.
#[derive(Clone, Debug)]
struct Frame {
    columns: Vec<Column>,
    before_x: f32,
    after_x: f32,
}

fn look(shot: &mut Snapshot) -> Frame {
    Frame {
        columns: columns(shot),
        before_x: label_area(shot, "Before").min_x(),
        after_x: label_area(shot, "After").min_x(),
    }
}

/// `frames` frames from now, the first after one step.
fn run(shot: &mut Snapshot, frames: usize) -> Vec<Frame> {
    (0..frames)
        .map(|_| {
            frame(shot);
            look(shot)
        })
        .collect()
}

/// The `n`th column from the right of a frame (0 is the units).
fn place(frame: &Frame, n: usize) -> &Column {
    &frame.columns[frame.columns.len() - 1 - n]
}

/// The height of one line of `style`, read where the app's size class is.
fn line(shot: &mut Snapshot, style: TextStyle) -> f32 {
    shot.runner().run_in(|| line_height(TextStyle { tabular: true, ..style }))
}

/// The width of a digit of `style`.
fn advance(shot: &mut Snapshot, style: TextStyle) -> f32 {
    shot.runner().run_in(|| digit_advance(TextStyle { tabular: true, ..style }))
}

#[test]
fn at_rest_it_shows_its_text_in_order() {
    let mut scene = Scene::new(&[1234.5]);
    scene.decimals = 1;
    scene.grouped = true;
    let mut shot = shot(scene);
    let frame = look(&mut shot);
    assert_eq!(rest_text(&frame.columns), "1,234.5");
    assert!(
        frame.columns.windows(2).all(|pair| pair[0].x + pair[0].width <= pair[1].x + 0.01),
        "columns follow each other left to right with no overlap"
    );
    for column in &frame.columns {
        assert_eq!(column.glyphs.len(), 1, "one glyph per column at rest");
    }
}

#[test]
fn only_the_digits_that_change_move_and_they_roll_up() {
    let mut shot = shot(Scene::new(&[129.0, 130.0]));
    let before = look(&mut shot);
    assert_eq!(rest_text(&before.columns), "129");
    let height = before.columns[0].height;
    tap(&mut shot);
    let frames = run(&mut shot, ROLL_FRAMES + 4);
    for frame in &frames {
        let hundreds = place(frame, 2);
        assert_eq!(hundreds.glyphs.len(), 1, "the unchanged digit is a single glyph");
        let seen = &hundreds.glyphs[0];
        assert_eq!((seen.ch, seen.y, seen.opacity), ('1', 0.0, 1.0), "the unchanged digit stays still");
    }
    let ones: Vec<&Column> = frames.iter().map(|frame| place(frame, 0)).collect();
    // The old nine leaves upwards, never back down.
    let nine: Vec<f32> = ones.iter().filter_map(|column| column.glyph('9')).map(|seen| seen.y).collect();
    assert!(nine.len() >= 3, "the old digit is seen for some frames: {nine:?}");
    assert!(nine.windows(2).all(|pair| pair[1] <= pair[0] + 0.01), "nine only goes up: {nine:?}");
    assert!(*nine.last().unwrap() < -height * 0.3, "and it leaves a good way up: {nine:?}");
    // The new zero rises from below and never sinks.
    let zero: Vec<f32> = ones.iter().filter_map(|column| column.glyph('0')).map(|seen| seen.y).collect();
    assert!(zero.windows(2).all(|pair| pair[1] <= pair[0] + 0.01), "zero only rises: {zero:?}");
    assert!(zero[0] > 0.0, "it starts below its place: {zero:?}");
    assert!(zero.iter().any(|y| *y > 0.5 && *y < height - 0.5), "and passes the places between");
    // Both are in the window at once, and they cross-fade.
    let both = ones.iter().find(|column| column.glyph('9').is_some() && column.glyph('0').is_some());
    let both = both.expect("a frame with both digits");
    let (old, new) = (both.glyph('9').unwrap(), both.glyph('0').unwrap());
    assert!(old.y < 0.0 && new.y > 0.0, "the old digit is above and the new one below");
    assert!((old.opacity + new.opacity - 1.0).abs() < 0.02, "they cross-fade: {both:?}");
    // The tens digit 2 becomes 3 the same way.
    let tens: Vec<&Column> = frames.iter().map(|frame| place(frame, 1)).collect();
    assert!(tens.iter().any(|column| column.glyph('2').is_some() && column.glyph('3').is_some()));
    // And everything ends at rest, exactly.
    let last = frames.last().unwrap();
    assert_eq!(rest_text(&last.columns), "130");
    for column in &last.columns {
        assert_eq!(column.glyphs.len(), 1, "{column:?}");
        assert!(column.glyphs[0].y.abs() < 1e-3 && (column.glyphs[0].opacity - 1.0).abs() < 1e-6);
    }
}

#[test]
fn going_down_rolls_the_other_way() {
    let mut shot = shot(Scene::new(&[130.0, 129.0]));
    tap(&mut shot);
    let frames = run(&mut shot, ROLL_FRAMES + 2);
    let ones: Vec<&Column> = frames.iter().map(|frame| place(frame, 0)).collect();
    let zero: Vec<f32> = ones.iter().filter_map(|column| column.glyph('0')).map(|seen| seen.y).collect();
    assert!(zero.windows(2).all(|pair| pair[1] >= pair[0] - 0.01), "the old zero only sinks: {zero:?}");
    assert!(*zero.last().unwrap() > 0.0);
    let nine: Vec<f32> = ones.iter().filter_map(|column| column.glyph('9')).map(|seen| seen.y).collect();
    assert!(nine[0] < 0.0, "the new nine starts above its place: {nine:?}");
    assert!(nine.windows(2).all(|pair| pair[1] >= pair[0] - 0.01), "and comes down: {nine:?}");
    assert_eq!(rest_text(&frames.last().unwrap().columns), "129");
}

#[test]
fn counting_does_not_change_the_width_or_move_the_neighbours() {
    let values: Vec<f64> = (100..=140).map(f64::from).collect();
    let mut shot = shot(Scene::new(&values));
    let first = look(&mut shot);
    let mut seen = Vec::new();
    for _ in 0..values.len() - 1 {
        tap(&mut shot);
        seen.extend(run(&mut shot, 2));
    }
    seen.extend(run(&mut shot, ROLL_FRAMES));
    for frame in &seen {
        assert_eq!(frame.before_x, first.before_x);
        assert!(
            (frame.after_x - first.after_x).abs() < 0.01,
            "the text after the number stays at {} and was at {}",
            first.after_x,
            frame.after_x
        );
        assert_eq!(frame.columns.iter().map(|column| column.width).sum::<f32>().round(), (first.columns.iter().map(|column| column.width).sum::<f32>()).round());
    }
    assert_eq!(rest_text(&seen.last().unwrap().columns), "140");
}

#[test]
fn a_new_leading_digit_grows_the_width_smoothly() {
    let mut shot = shot(Scene::new(&[99.0, 100.0]));
    let before = look(&mut shot);
    let advance = advance(&mut shot, NUMERIC);
    tap(&mut shot);
    let frames = run(&mut shot, ROLL_FRAMES + 2);
    let xs: Vec<f32> = frames.iter().map(|frame| frame.after_x).collect();
    assert!(xs.windows(2).all(|pair| pair[1] >= pair[0] - 0.01), "never shrinks on the way: {xs:?}");
    assert!(xs.iter().any(|x| *x > before.after_x + 0.5 && *x < before.after_x + advance - 0.5), "{xs:?}");
    assert!((xs.last().unwrap() - (before.after_x + advance)).abs() < 0.01, "ends one digit wider: {xs:?}");
    // The hundreds digit rises in from below while it fades in.
    let hundreds: Vec<Seen> = frames
        .iter()
        .filter_map(|frame| frame.columns.iter().find_map(|column| column.glyph('1')))
        .cloned()
        .collect();
    assert!(hundreds.windows(2).all(|pair| pair[1].y <= pair[0].y + 0.01 && pair[1].opacity >= pair[0].opacity - 0.01));
    assert!(hundreds[0].y > 0.0 && hundreds[0].opacity < 1.0, "{:?}", hundreds[0]);
    assert_eq!(rest_text(&frames.last().unwrap().columns), "100");
}

#[test]
fn a_lost_leading_digit_shrinks_the_width_smoothly() {
    let mut shot = shot(Scene::new(&[100.0, 99.0]));
    let before = look(&mut shot);
    let advance = advance(&mut shot, NUMERIC);
    tap(&mut shot);
    let frames = run(&mut shot, ROLL_FRAMES + 2);
    let xs: Vec<f32> = frames.iter().map(|frame| frame.after_x).collect();
    assert!(xs.windows(2).all(|pair| pair[1] <= pair[0] + 0.01), "never grows on the way: {xs:?}");
    assert!(xs.iter().any(|x| *x < before.after_x - 0.5 && *x > before.after_x - advance + 0.5), "{xs:?}");
    assert!((xs.last().unwrap() - (before.after_x - advance)).abs() < 0.01, "ends one digit narrower: {xs:?}");
    assert_eq!(rest_text(&frames.last().unwrap().columns), "99");
}

#[test]
fn a_column_that_came_back_is_reused() {
    let mut shot = shot(Scene::new(&[9.0, 10.0, 9.0, 10.0]));
    for _ in 0..3 {
        tap(&mut shot);
        frames_at_rest(&mut shot);
    }
    assert_eq!(rest_text(&columns(&mut shot)), "10");
}

/// Counts 0 to 9 with a tap every `gap` frames, and returns every frame of the
/// units column, then the frames of the wait after the last tap.
fn count_with_gap(shot: &mut Snapshot, gap: usize) -> (Vec<Frame>, Vec<Frame>) {
    let mut frames = Vec::new();
    for _ in 0..9 {
        tap(shot);
        frames.extend(run(shot, gap));
    }
    let tail = run(shot, ROLL_FRAMES + 1);
    (frames, tail)
}

/// What the units column draws in each frame, for a failure message.
fn series(frames: &[&Frame]) -> Vec<String> {
    frames
        .iter()
        .map(|frame| {
            place(frame, 0)
                .glyphs
                .iter()
                .map(|glyph| format!("{}@{:.1}", glyph.ch, glyph.y))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect()
}

#[test]
fn rapid_updates_continue_from_where_the_digits_are() {
    let values: Vec<f64> = (0..=9).map(f64::from).collect();
    let mut shot = shot(Scene::new(&values));
    let height = line(&mut shot, NUMERIC);
    // A value every two animation steps.
    let (frames, tail) = count_with_gap(&mut shot, 4);
    // No glyph jumps: between two frames a digit moves less than 60 % of a line.
    let all: Vec<&Frame> = frames.iter().chain(tail.iter()).collect();
    let mut worst = 0.0_f32;
    for pair in all.windows(2) {
        let (a, b) = (place(pair[0], 0), place(pair[1], 0));
        for glyph in &a.glyphs {
            if let Some(next) = b.glyph(glyph.ch) {
                worst = worst.max((next.y - glyph.y).abs());
            }
        }
    }
    assert!(worst < height * 0.6, "a digit jumped {worst} px of a {height} px line: {:#?}", series(&all));
    // Nothing queued: one roll after the last value, and it is at rest.
    let last = tail.last().unwrap();
    assert_eq!(rest_text(&last.columns), "9");
    assert_eq!(place(last, 0).glyphs.len(), 1);
}

#[test]
fn a_value_on_every_animation_step_only_ever_moves_the_digits_on() {
    let values: Vec<f64> = (0..=9).map(f64::from).collect();
    let mut shot = shot(Scene::new(&values));
    // A new value on every step of the clock, the fastest a number can be fed.
    let (frames, tail) = count_with_gap(&mut shot, 2);
    let all: Vec<&Frame> = frames.iter().chain(tail.iter()).collect();
    for pair in all.windows(2) {
        let (a, b) = (place(pair[0], 0), place(pair[1], 0));
        for glyph in &a.glyphs {
            if let Some(next) = b.glyph(glyph.ch) {
                assert!(
                    next.y <= glyph.y + 0.01,
                    "{} turned back from {} to {}: {:#?}",
                    glyph.ch,
                    glyph.y,
                    next.y,
                    series(&all)
                );
            }
        }
    }
    // The queue is empty: the last value is at rest a roll after it came, not
    // nine rolls after.
    let last = tail.last().unwrap();
    assert_eq!(rest_text(&last.columns), "9");
    assert_eq!(place(last, 0).glyphs.len(), 1);
}

#[test]
fn a_value_that_comes_back_mid_roll_turns_around_where_it_is() {
    let mut shot = shot(Scene::new(&[5.0, 6.0, 5.0]));
    let height = line(&mut shot, NUMERIC);
    tap(&mut shot);
    let mut frames = run(&mut shot, 3);
    tap(&mut shot);
    frames.extend(run(&mut shot, ROLL_FRAMES + 2));
    let mut worst = 0.0_f32;
    for pair in frames.windows(2) {
        let (a, b) = (place(&pair[0], 0), place(&pair[1], 0));
        for glyph in &a.glyphs {
            if let Some(next) = b.glyph(glyph.ch) {
                worst = worst.max((next.y - glyph.y).abs());
            }
        }
    }
    assert!(worst < height * 0.6, "a digit jumped {worst} px");
    let last = frames.last().unwrap();
    assert_eq!(rest_text(&last.columns), "5");
    assert_eq!(place(last, 0).glyphs.len(), 1);
}

#[test]
fn signs_and_separators_cross_fade_in_place_and_only_digits_move() {
    // A sign that goes.
    let mut shot_sign = shot(Scene::new(&[-5.0, 5.0]));
    assert_eq!(rest_text(&look(&mut shot_sign).columns), "\u{2212}5");
    tap(&mut shot_sign);
    let frames = run(&mut shot_sign, ROLL_FRAMES);
    let signs: Vec<&Seen> = frames
        .iter()
        .filter_map(|frame| frame.columns.iter().find_map(|column| column.glyph('\u{2212}')))
        .collect();
    assert!(!signs.is_empty());
    assert!(signs.iter().all(|seen| seen.y == 0.0), "the sign does not travel");
    assert!(signs.windows(2).all(|pair| pair[1].opacity <= pair[0].opacity + 0.01), "it fades out");
    for frame in &frames {
        let ones = place(frame, 0);
        assert_eq!(ones.glyphs.len(), 1, "the five does not change, so it does not move");
        assert_eq!(ones.glyphs[0].y, 0.0);
    }
    assert_eq!(rest_text(&frames.last().unwrap().columns), "5");

    // A separator that comes.
    let mut scene = Scene::new(&[999.0, 1000.0]);
    scene.grouped = true;
    let mut shot_group = shot(scene);
    tap(&mut shot_group);
    let frames = run(&mut shot_group, ROLL_FRAMES + 2);
    let commas: Vec<&Seen> = frames
        .iter()
        .filter_map(|frame| frame.columns.iter().find_map(|column| column.glyph(',')))
        .collect();
    assert!(!commas.is_empty());
    assert!(commas.iter().all(|seen| seen.y == 0.0), "the comma does not travel");
    assert!(commas.windows(2).all(|pair| pair[1].opacity >= pair[0].opacity - 0.01), "it fades in");
    assert_eq!(rest_text(&frames.last().unwrap().columns), "1,000");
}

#[test]
fn reduced_motion_is_an_instant_change() {
    let mut scene = Scene::new(&[129.0, 130.0]);
    scene.reduced = true;
    let mut shot = shot(scene);
    tap(&mut shot);
    let frames = run(&mut shot, 3);
    let last = frames.last().unwrap();
    assert_eq!(rest_text(&last.columns), "130");
    for column in &last.columns {
        assert_eq!(column.glyphs.len(), 1);
        assert_eq!(column.glyphs[0].y, 0.0);
    }
    // No frame has two digits in one column.
    assert!(frames.iter().skip(1).all(|frame| frame.columns.iter().all(|column| column.glyphs.len() == 1)));
}

#[test]
fn units_stay_where_they_are() {
    let mut scene = Scene::new(&[129.0, 130.0]);
    scene.prefix = Some("$");
    scene.suffix = Some("%");
    let mut shot = shot(scene);
    let dollar = label_area(&mut shot, "$").min_x();
    let percent = label_area(&mut shot, "%").min_x();
    tap(&mut shot);
    for _ in 0..ROLL_FRAMES {
        frame(&mut shot);
        assert_eq!(label_area(&mut shot, "$").min_x(), dollar);
        assert!((label_area(&mut shot, "%").min_x() - percent).abs() < 0.01, "the unit does not move while digits roll");
    }
}

#[test]
fn it_sits_on_the_baseline_of_the_text_beside_it_at_every_size() {
    for style in [NUMERIC, HERO, CAPTION, crate::typography::PAGE_TITLE] {
        let mut shot = shot_sized(Scene { style, ..Scene::new(&[428.0]) }, (520.0, 120.0));
        let frame = look(&mut shot);
        let before = label_area(&mut shot, "Before");
        let digit = frame.columns[0].glyphs[0].clone();
        assert_eq!(digit.top, before.min_y(), "the digit's box starts where the neighbour's does");
        assert!((digit.y).abs() < 1e-3);
        for column in &frame.columns {
            assert_eq!(column.top, before.min_y());
            assert!(
                (column.height - before.height()).abs() < 0.51,
                "the window is one line: {} against the label's {}",
                column.height,
                before.height()
            );
        }
    }
}

#[test]
fn digits_are_tabular() {
    let values: Vec<f64> = (0..=9).map(f64::from).collect();
    let mut shot = shot(Scene::new(&values));
    let mut widths = vec![look(&mut shot).columns[0].glyphs[0].width];
    for _ in 1..=9 {
        tap(&mut shot);
        frames_at_rest(&mut shot);
        widths.push(look(&mut shot).columns[0].glyphs[0].width);
    }
    assert!(
        widths.iter().all(|width| (width - widths[0]).abs() < 0.01),
        "every digit has the same advance: {widths:?}"
    );
}

#[test]
fn assistive_technology_gets_the_final_value_once_and_politely() {
    fn status(shot: &mut Snapshot) -> Vec<(String, Option<String>, Option<Live>)> {
        shot.runner().find_many(|_, element| {
            let data = element.accessibility();
            (data.builder.role() == Role::Status).then(|| {
                (
                    data.builder.label().unwrap_or_default().to_owned(),
                    data.builder.value().map(str::to_owned),
                    data.builder.live(),
                )
            })
        })
    }
    let mut scene = Scene::new(&[129.0, 130.0]);
    scene.suffix = Some("%");
    let mut shot = shot(scene);
    assert_eq!(
        status(&mut shot),
        vec![("129%".to_owned(), Some("129%".to_owned()), Some(Live::Polite))]
    );
    tap(&mut shot);
    let mut labels = Vec::new();
    for _ in 0..ROLL_FRAMES + 2 {
        frame(&mut shot);
        let now = status(&mut shot);
        assert_eq!(now.len(), 1);
        labels.push(now[0].0.clone());
    }
    labels.dedup();
    assert_eq!(labels, vec!["130%".to_owned()], "the value is the final one from the first frame, never an in-between");
    // The glyphs are not read one by one.
    let hidden = shot
        .runner()
        .find_many(|_, element| element.accessibility().builder.is_hidden().then_some(()))
        .len();
    assert!(hidden >= 4, "three columns and the unit are hidden from the tree: {hidden}");

    let mut quiet = Scene::new(&[129.0, 130.0]);
    quiet.silent = true;
    let mut shot = shot_sized(quiet, SIZE);
    assert!(status(&mut shot).is_empty(), "a silent number is not a live region");
    let labelled: Vec<(Role, Option<Live>)> = shot.runner().find_many(|_, element| {
        let data = element.accessibility();
        (data.builder.value() == Some("129")).then(|| (data.builder.role(), data.builder.live()))
    });
    assert!(labelled.contains(&(Role::Label, None)), "{labelled:?}");
}

fn dir() -> Option<PathBuf> {
    std::env::var_os("BINGUX_FRAME_DIR").map(PathBuf::from)
}

/// Mount `scene`, tap `taps` times with `gap` frames between, and save
/// `frames` frames in all (the first is the state before the first tap).
fn record(name: &str, scene: Scene, size: (f32, f32), taps: usize, gap: usize, frames: usize) {
    let Some(dir) = dir() else { return };
    let mut shot = shot_sized(scene, size);
    shot.save(dir.join(format!("{name}-00.png")));
    let mut next_tap = 0;
    for index in 1..=frames {
        if taps > 0 && (index - 1) % gap == 0 && next_tap < taps {
            tap(&mut shot);
            next_tap += 1;
        }
        frame(&mut shot);
        shot.save(dir.join(format!("{name}-{index:02}.png")));
    }
}

#[test]
#[ignore = "writes frame strips when BINGUX_FRAME_DIR is set"]
fn frame_strips() {
    let count = |values: &[f64]| Scene { suffix: Some(" left"), ..Scene::new(values) };
    record("up", count(&[129.0, 130.0]), SIZE, 1, 1, 14);
    record("down", count(&[130.0, 129.0]), SIZE, 1, 1, 14);
    record("grow", count(&[99.0, 100.0]), SIZE, 1, 1, 14);
    record("shrink", count(&[100.0, 99.0]), SIZE, 1, 1, 14);
    record("percent", Scene { suffix: Some("%"), ..Scene::new(&[87.0, 88.0, 89.0]) }, SIZE, 2, 6, 24);
    record(
        "speed",
        Scene { decimals: 1, suffix: Some(" MB/s"), ..Scene::new(&[9.9, 10.1]) },
        SIZE,
        1,
        1,
        14,
    );
    record("signed", Scene::new(&[-1.0, 0.0, 1.0]), SIZE, 2, 8, 24);
    record(
        "big",
        Scene { style: HERO, grouped: true, ..Scene::new(&[1999.0, 2000.0]) },
        (520.0, 100.0),
        1,
        1,
        14,
    );
    record("small", Scene { style: CAPTION, ..Scene::new(&[41.0, 42.0]) }, SIZE, 1, 1, 14);
    record("rapid", Scene::new(&(0..=9).map(f64::from).collect::<Vec<_>>()), SIZE, 9, 2, 32);
    record("reversal", Scene::new(&[5.0, 6.0, 5.0]), SIZE, 2, 4, 20);
    record(
        "phone",
        Scene { style: HERO, suffix: Some("%"), ..Scene::new(&[58.0, 59.0]) },
        (390.0, 100.0),
        1,
        1,
        14,
    );
}
