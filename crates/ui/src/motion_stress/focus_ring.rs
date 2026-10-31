//! The keyboard focus ring as focus is moved quickly: one ring at rest, never more than two on the
//! way, always around its control, and nothing else of the window moves for it.

use freya::prelude::*;

use super::*;
use crate::controls::{action_button, checkbox, switch};
use crate::style::*;
use crate::surface::use_tokens;

const WINDOW: (f32, f32) = (700.0, 300.0);
const EPS: f32 = 0.005;
/// How far outside its face a ring reaches at most: the gap, the line, the travel and the blur.
/// The controls of the scene.
const CONTROLS: usize = 5;
const REACH: f32 = 2.0 + 2.0 + 4.0 + 4.0 + 1.0;

#[derive(Clone, PartialEq)]
struct Scene;

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut a = use_state(|| false);
        let mut b = use_state(|| true);
        let mut c = use_state(|| false);
        WindowHost::new(
            rect()
                .expanded()
                .tw(
                    &tokens,
                    &[
                        bg(Paint::Bg),
                        p(S8),
                        flex_row(),
                        gap(S8),
                        items(Align::Center),
                    ],
                )
                .child(action_button("First", |_| {}))
                .child(switch(a(), move |next| a.set(next)))
                .child(checkbox(b(), move |next| b.set(next)))
                .child(action_button("Last", |_| {}))
                .child(switch(c(), move |next| c.set(next))),
        )
    }
}

/// Each ring of a frame: its box, its folded opacity and its face's box.
fn rings(frame: &Frame) -> Vec<(Area, f32, Area)> {
    frame
        .all("FocusRing")
        .into_iter()
        .filter_map(|ring| {
            let face = frame.node(ring.parent?).area?;
            Some((ring.area?, frame.opacity(ring.id), face))
        })
        .collect()
}

fn visible(frame: &Frame) -> Vec<(Area, f32, Area)> {
    rings(frame)
        .into_iter()
        .filter(|(_, opacity, _)| *opacity > 0.02)
        .collect()
}

fn every_frame(frames: &[Frame], findings: &mut Findings) {
    for (at, frame) in frames.iter().enumerate() {
        for (ring, opacity, face) in rings(frame) {
            findings.check((-EPS..=1.0 + EPS).contains(&opacity), || {
                format!("frame {at}: a ring with opacity {opacity:.3}")
            });
            // The ring surrounds its face and reaches no further than its motion allows.
            findings.check(
                ring.min_x() <= face.min_x() + 0.5
                    && ring.min_y() <= face.min_y() + 0.5
                    && ring.max_x() >= face.max_x() - 0.5
                    && ring.max_y() >= face.max_y() - 0.5,
                || format!("frame {at}: the ring {ring:?} does not surround its face {face:?}"),
            );
            findings.check(
                face.min_x() - ring.min_x() <= REACH
                    && ring.max_x() - face.max_x() <= REACH
                    && face.min_y() - ring.min_y() <= REACH
                    && ring.max_y() - face.max_y() <= REACH,
                || format!("frame {at}: the ring {ring:?} reaches more than {REACH} px from its face {face:?}"),
            );
        }
        // Never more rings than controls.
        findings.check(visible(frame).len() <= CONTROLS, || {
            format!(
                "frame {at}: {} rings are drawn, there are {CONTROLS} controls",
                visible(frame).len()
            )
        });
    }
    // The ring never moves a box of the window.
    findings.extend(
        unmoved(frames, 0.01, |path| {
            path.contains("FocusRing") || path.contains("FocusBand")
        })
        .into_iter()
        .map(Err),
    );
}

fn tab(rig: &mut Rig, shift: bool) {
    if shift {
        rig.key_with(Key::Named(NamedKey::Tab), Modifiers::SHIFT);
    } else {
        rig.key(Key::Named(NamedKey::Tab));
    }
}

#[test]
fn tabbing_through_the_controls_leaves_one_whole_ring_at_the_last_of_them() {
    Env::each(&Env::ALL, |env, findings| {
        for gap in [0usize, 1, 3, 12] {
            let mut rig = Rig::new(env, WINDOW, Scene);
            for _ in 0..5 {
                tab(&mut rig, false);
                rig.steps(gap);
            }
            rig.steps(24);
            every_frame(&rig.frames, findings);
            // A tab that comes after the last one's ring has gone out (120 ms) leaves two rings at
            // most: the old going out and the new coming in.
            if gap >= 8 {
                for (at, frame) in rig.frames.iter().enumerate() {
                    findings.check(visible(frame).len() <= 2, || {
                        format!(
                            "{gap} frames apart, frame {at}: {} rings at once",
                            visible(frame).len()
                        )
                    });
                }
            }
            let now = visible(rig.now());
            findings.check(now.len() == 1, || {
                format!("{gap} frames apart: {} rings at rest", now.len())
            });
            if let Some((_, opacity, _)) = now.first() {
                findings.check(*opacity >= 1.0 - EPS, || {
                    format!("{gap} frames apart: the ring at rest is {opacity:.3} opaque")
                });
            }
        }
    });
}

#[test]
fn tabbing_back_and_forth_on_every_frame_keeps_to_two_rings_and_ends_on_the_right_control() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        for round in 0..10 {
            tab(&mut rig, round % 3 == 2);
            rig.step();
        }
        rig.steps(24);
        every_frame(&rig.frames, findings);
        // Seven forward and three back: the fourth stop of five from the first.
        let now = visible(rig.now());
        findings.check(now.len() == 1, || format!("{} rings at rest", now.len()));
        let mut calm = Rig::new(env, WINDOW, Scene);
        for round in 0..10 {
            tab(&mut calm, round % 3 == 2);
            calm.steps(12);
        }
        calm.steps(12);
        let (a, b) = (visible(rig.now()), visible(calm.now()));
        findings.check(
            a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| x.2 == y.2),
            || {
                format!(
                    "the ring is on {:?}, a calm run puts it on {:?}",
                    a.iter().map(|r| r.2).collect::<Vec<_>>(),
                    b.iter().map(|r| r.2).collect::<Vec<_>>()
                )
            },
        );
    });
}

#[test]
fn a_ring_that_arrives_and_is_sent_away_half_way_goes_out_from_where_it_is() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        tab(&mut rig, false);
        rig.steps(3);
        let turn = rig.frames.len();
        let before = visible(rig.now()).first().map_or(0.0, |r| r.1);
        // Away from the keyboard: the pointer press drops keyboard focus mode.
        rig.click_at(WINDOW.0 - 20.0, WINDOW.1 - 20.0);
        rig.steps(20);
        every_frame(&rig.frames, findings);
        let os: Vec<f32> = rig
            .frames
            .iter()
            .map(|f| rings(f).into_iter().map(|r| r.1).fold(0.0, f32::max))
            .collect();
        findings.extend([monotone_down(
            "the ring's opacity after the press",
            &os[turn - 1..],
            EPS,
        )]);
        findings.check(os[turn] <= before + EPS, || {
            format!(
                "the ring leapt from {before:.2} to {:.2} when the pointer pressed",
                os[turn]
            )
        });
        findings.extend([ends_at("the ring's opacity", &os, 0.0, EPS)]);
    });
}

#[test]
fn reduced_motion_shows_and_drops_the_ring_at_once() {
    Env::each(&Env::REDUCED, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        tab(&mut rig, false);
        rig.steps(6);
        tab(&mut rig, false);
        rig.steps(6);
        every_frame(&rig.frames, findings);
        for (at, frame) in rig.frames.iter().enumerate() {
            for (_, opacity, _) in rings(frame) {
                findings.check(opacity <= EPS || opacity >= 1.0 - EPS, || {
                    format!("frame {at}: reduced motion drew a ring {opacity:.2} opaque")
                });
            }
        }
    });
}
