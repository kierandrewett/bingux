//! The tooltip as the pointer comes and goes: it waits its turn, fades in where it stays, never
//! flashes for a pointer that is only passing, and leaves at once when its control is pressed.
//!
//! The wait before a tip is the controls' own clock (500 ms of real time), so a test that needs the
//! tip steps the window until it is there; the frames it checks are the rig's fixed 16 ms steps.

use freya::prelude::*;

use super::*;
use crate::controls::icon_button;
use crate::icons::Icon;
use crate::style::*;
use crate::surface::use_tokens;

const WINDOW: (f32, f32) = (700.0, 300.0);
const EPS: f32 = 0.005;
/// How many steps (40 ms of real time each) a test gives the tip to come: far more than 500 ms.
const PATIENCE: usize = 60;
const TIPS: [&str; 3] = [
    "Refresh the list",
    "Go to the parent folder",
    "Create a new folder",
];

#[derive(Clone, PartialEq)]
struct Scene;

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        WindowHost::new(
            rect()
                .expanded()
                .tw(&tokens, &[bg(Paint::Bg), p(S8), flex_row(), gap(S8)])
                .child(icon_button(Icon::Refresh, |_| {}).tooltip(TIPS[0]))
                .child(icon_button(Icon::PanUp, |_| {}).tooltip(TIPS[1]))
                .child(icon_button(Icon::FolderNew, |_| {}).tooltip(TIPS[2])),
        )
    }
}

/// The most visible bubble of `text` in a frame: its box and folded opacity, `None` when no copy
/// of the text is drawn.
fn bubble(frame: &Frame, text: &str) -> Option<(Area, f32)> {
    frame
        .texts()
        .into_iter()
        .filter(|look| look.text == text)
        .map(|look| (look.area, frame.opacity(look.id)))
        .filter(|(_, opacity)| *opacity > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

fn button_area(frame: &Frame, index: usize) -> Area {
    frame
        .nth("IconButton", index)
        .area
        .expect("a button has an area")
}

fn hover_button(rig: &mut Rig, index: usize) {
    let area = button_area(rig.now(), index);
    rig.hover_at(
        area.min_x() + area.width() / 2.0,
        area.min_y() + area.height() / 2.0,
    );
}

/// Steps until the bubble of `text` shows at all, at most [`PATIENCE`] steps.
fn wait_for(rig: &mut Rig, text: &str) -> bool {
    for _ in 0..PATIENCE {
        if bubble(rig.now(), text).is_some() {
            return true;
        }
        rig.step();
    }
    false
}

#[test]
fn a_tip_fades_in_on_one_spot_inside_the_window_off_its_control() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        hover_button(&mut rig, 0);
        findings.check(wait_for(&mut rig, TIPS[0]), || {
            "the tip never came".to_owned()
        });
        rig.steps(16);
        let seen: Vec<(Area, f32)> = rig
            .frames
            .iter()
            .filter_map(|f| bubble(f, TIPS[0]))
            .collect();
        let os: Vec<f32> = seen.iter().map(|s| s.1).collect();
        findings.extend([monotone_up("tip opacity", &os, EPS)]);
        findings.extend([ends_at("tip opacity", &os, 1.0, EPS)]);
        // One place from its first frame: it fades, it does not travel.
        let xs: Vec<f32> = seen.iter().map(|s| s.0.min_x()).collect();
        let ys: Vec<f32> = seen.iter().map(|s| s.0.min_y()).collect();
        findings.extend([constant("tip x", &xs, EPS)]);
        findings.extend([constant("tip y", &ys, EPS)]);
        // Inside the window, clear of its button.
        let (area, _) = *seen.last().expect("the tip showed");
        let button = button_area(rig.now(), 0);
        findings.check(
            area.min_x() >= 0.0 && area.max_x() <= WINDOW.0 && area.max_y() <= WINDOW.1,
            || format!("the tip {area:?} leaves the window"),
        );
        findings.check(area.intersection(&button).is_none(), || {
            format!("the tip {area:?} covers its button {button:?}")
        });
        // The fade has the tooltip's own length, at 16 ms a frame.
        // The series starts on the first frame the tip is drawn: its frames up to the whole one.
        let frames = rest_index(&os, 0.01) + 1;
        let wanted = 120u64.div_ceil(STEP_MS) as usize;
        findings.check((wanted - 1..=wanted + 1).contains(&frames), || {
            format!(
                "the fade took {frames} frames, 120 ms is {wanted}: {}",
                show(&os)
            )
        });
    });
}

#[test]
fn a_pointer_that_comes_and_goes_every_frame_shows_no_tip_at_all() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        let away = (WINDOW.0 - 10.0, WINDOW.1 - 10.0);
        for step in 0..24 {
            if step % 2 == 0 {
                hover_button(&mut rig, 0);
            } else {
                rig.hover_at(away.0, away.1);
            }
            rig.step();
        }
        for (at, frame) in rig.frames.iter().enumerate() {
            for tip in TIPS {
                findings.check(bubble(frame, tip).is_none(), || {
                    format!("frame {at}: the tip {tip:?} flashed for a pointer that was passing")
                });
            }
        }
    });
}

#[test]
fn pressing_the_control_neither_moves_nor_flickers_the_tip() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        hover_button(&mut rig, 1);
        findings.check(wait_for(&mut rig, TIPS[1]), || {
            "the tip never came".to_owned()
        });
        rig.steps(10);
        let from = rig.frames.len();
        let before = bubble(rig.now(), TIPS[1]).expect("the tip is up").0;
        let area = button_area(rig.now(), 1);
        rig.click_at(
            area.min_x() + area.width() / 2.0,
            area.min_y() + area.height() / 2.0,
        );
        rig.steps(20);
        // A press keeps the tip where it was, whole, for as long as the pointer stays: no frame
        // dims it or moves it.
        for (at, frame) in rig.frames[from..].iter().enumerate() {
            match bubble(frame, TIPS[1]) {
                Some((area, opacity)) => {
                    findings.check(area == before, || {
                        format!("frame {at}: the tip moved {before:?} -> {area:?}")
                    });
                    findings.check(opacity >= 1.0 - EPS, || {
                        format!("frame {at}: the tip dimmed to {opacity:.2}")
                    });
                }
                None => findings.add(format!("frame {at}: the tip vanished on a press")),
            }
        }
    });
}

#[test]
fn moving_to_the_next_control_shows_its_tip_and_never_two_at_full_strength() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        hover_button(&mut rig, 0);
        findings.check(wait_for(&mut rig, TIPS[0]), || {
            "the first tip never came".to_owned()
        });
        rig.steps(10);
        let from = rig.frames.len();
        hover_button(&mut rig, 1);
        rig.steps(30);
        for (at, frame) in rig.frames[from..].iter().enumerate() {
            let strong: Vec<&str> = TIPS
                .iter()
                .copied()
                .filter(|tip| bubble(frame, tip).is_some_and(|s| s.1 > 0.5))
                .collect();
            findings.check(strong.len() <= 1, || {
                format!("frame {at}: {strong:?} are all more than half drawn")
            });
        }
        let os: Vec<f32> = rig.frames[from..]
            .iter()
            .map(|f| bubble(f, TIPS[1]).map_or(0.0, |s| s.1))
            .collect();
        findings.extend([ends_at("the second tip", &os, 1.0, EPS)]);
    });
}

#[test]
fn reduced_motion_shows_the_tip_whole_when_it_comes() {
    Env::each(&Env::REDUCED, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        hover_button(&mut rig, 0);
        findings.check(wait_for(&mut rig, TIPS[0]), || {
            "the tip never came".to_owned()
        });
        rig.steps(6);
        let os: Vec<f32> = rig
            .frames
            .iter()
            .filter_map(|f| bubble(f, TIPS[0]))
            .map(|s| s.1)
            .collect();
        for (at, o) in os.iter().enumerate() {
            findings.check(*o >= 1.0 - EPS, || {
                format!("frame {at}: reduced motion drew the tip {o:.2} opaque")
            });
        }
    });
}
