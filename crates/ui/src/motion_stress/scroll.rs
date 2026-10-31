//! A scroll view driven by the wheel: the content glides along one curve to where the wheel sent it,
//! a wheel turned back starts from where the content is, and it never leaves its ends.

use freya::prelude::*;

use super::*;
use crate::rows::row;
use crate::style::*;
use crate::surface::use_tokens;

const WINDOW: (f32, f32) = (700.0, 480.0);
const EPS: f32 = 0.01;
const ROWS: usize = 60;

#[derive(Clone, PartialEq)]
struct Scene;

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        WindowHost::new(
            rect().expanded().tw(&tokens, &[bg(Paint::Bg)]).child(
                ScrollView::new()
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(rect().width(Size::fill()).children((0..ROWS).map(|index| {
                        Element::from(row(format!("Scroll row {index}")).value("A value"))
                    }))),
            ),
        )
    }
}

fn row_y(frame: &Frame, index: usize) -> f32 {
    frame
        .texts()
        .iter()
        .find(|look| look.text == format!("Scroll row {index}"))
        .unwrap_or_else(|| panic!("row {index} is not on screen"))
        .area
        .min_y()
}

/// The content's offset: how far row 0 has moved from where it started (negative is scrolled).
fn offset(frame: &Frame, rest: f32) -> f32 {
    row_y(frame, 0) - rest
}

fn every_frame(frames: &[Frame], findings: &mut Findings) {
    // The rows keep their spacing while they move: one box, scrolled, not rows sliding past each other.
    for (at, frame) in frames.iter().enumerate() {
        let gaps: Vec<f32> = (0..4)
            .map(|i| row_y(frame, i + 1) - row_y(frame, i))
            .collect();
        findings.check(gaps.iter().all(|gap| (gap - gaps[0]).abs() <= 0.5), || {
            format!("frame {at}: the rows' spacing varies: {gaps:?}")
        });
    }
}

#[test]
fn one_wheel_turn_glides_to_where_it_was_sent_and_stops_there() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        let rest = row_y(rig.now(), 0);
        rig.wheel(350.0, 240.0, -200.0);
        rig.steps(40);
        every_frame(&rig.frames, findings);
        let ys: Vec<f32> = rig.frames.iter().map(|f| offset(f, rest)).collect();
        findings.extend([monotone_down("the content", &ys, EPS)]);
        // The wheel was a pixel event of 200 logical pixels.
        findings.extend([within("the content", &ys, -200.0, 0.0, EPS)]);
        findings.extend([ends_at("the content", &ys, -200.0, 0.5)]);
        // It does not wait: it has begun to move within two frames, and is done in a third of a second.
        findings.check(start_index(&ys, EPS) <= 2, || {
            format!(
                "the content began to move on frame {}",
                start_index(&ys, EPS)
            )
        });
        findings.check(rest_index(&ys, 0.5) <= 22, || {
            format!(
                "the content took {} frames to stop: {}",
                rest_index(&ys, 0.5),
                show(&ys)
            )
        });
    });
}

#[test]
fn a_burst_of_turns_never_overshoots_and_a_turn_back_starts_from_where_the_content_is() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        let rest = row_y(rig.now(), 0);
        for _ in 0..8 {
            rig.wheel(350.0, 240.0, -80.0);
            rig.step();
        }
        let turn = rig.frames.len();
        let here = offset(rig.now(), rest);
        rig.wheel(350.0, 240.0, 120.0);
        rig.steps(50);
        every_frame(&rig.frames, findings);
        let ys: Vec<f32> = rig.frames.iter().map(|f| offset(f, rest)).collect();
        // Down to the turn it never goes back; after it, it never goes down again.
        findings.extend([monotone_down(
            "the content before the turn",
            &ys[..turn],
            EPS,
        )]);
        findings.extend([monotone_up(
            "the content after the turn",
            &ys[turn - 1..],
            EPS,
        )]);
        // The turn starts where the content was and ends a turn's distance from it, not from the
        // place the first burst was heading for.
        findings.check((ys[turn] - here).abs() <= 120.0, || {
            format!("the content jumped {:.1} at the turn", ys[turn] - here)
        });
        findings.extend([ends_at(
            "the content after the turn",
            &ys,
            here + 120.0,
            1.5,
        )]);
        findings.extend([no_jump("the content", &ys, 150.0)]);
    });
}

#[test]
fn the_ends_hold_the_content_in_and_it_comes_back_whole() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        let rest = row_y(rig.now(), 0);
        // Above the top first: nothing moves.
        rig.wheel(350.0, 240.0, 300.0);
        rig.steps(12);
        let ys: Vec<f32> = rig.frames.iter().map(|f| offset(f, rest)).collect();
        findings.extend([constant("the content turned past its top", &ys, EPS)]);
        // Then a long way down, in several turns (the rows are mounted a screenful at a time).
        for _ in 0..12 {
            rig.wheel(350.0, 240.0, -4000.0);
            rig.steps(6);
        }
        rig.steps(40);
        let bottom = offset(rig.now(), rest);
        findings.check(bottom < -(ROWS as f32) * 30.0, || {
            format!("the content only got to {bottom:.0}")
        });
        let tail: Vec<f32> = rig.frames[rig.frames.len() - 20..]
            .iter()
            .map(|f| offset(f, rest))
            .collect();
        findings.extend([constant("the content at the bottom", &tail, EPS)]);
        // The last row's bottom is at the window's bottom: not past it, not short of it.
        let last = rig
            .now()
            .texts()
            .iter()
            .find(|look| look.text.starts_with("Scroll row "))
            .map(|l| l.area.min_y())
            .unwrap_or(0.0);
        findings.check(last < WINDOW.1, || {
            format!("the window shows nothing past y {last:.0}")
        });
        // And back to the top in one long turn: it ends where it started, exactly.
        for _ in 0..12 {
            rig.wheel(350.0, 240.0, 6000.0);
            rig.steps(6);
        }
        rig.steps(40);
        findings.check((offset(rig.now(), rest)).abs() <= EPS, || {
            format!(
                "back at the top the content is at {:.2}",
                offset(rig.now(), rest)
            )
        });
    });
}

#[test]
fn the_pointer_crossing_the_rows_while_the_content_glides_moves_nothing() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        let rest = row_y(rig.now(), 0);
        rig.wheel(350.0, 240.0, -300.0);
        for step in 0..16 {
            rig.hover_at(100.0 + step as f32 * 20.0, 60.0 + (step % 4) as f32 * 90.0);
            rig.step();
        }
        rig.steps(20);
        every_frame(&rig.frames, findings);
        let ys: Vec<f32> = rig.frames.iter().map(|f| offset(f, rest)).collect();
        findings.extend([monotone_down("the content", &ys, EPS)]);
        findings.extend([ends_at("the content", &ys, -300.0, 0.5)]);
    });
}
