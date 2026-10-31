//! The slider, frame by frame: the knob follows the pointer and the keys, sits on the track's
//! line, grows and shrinks along one curve, and nothing else of the window moves.

use freya::prelude::*;

use super::*;
use crate::controls::slider;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::BODY;

const WINDOW: (f32, f32) = (900.0, 300.0);
/// What the knob grows to while hovered, held and dragged, from `controls/slider.rs`.
const HOVER: f32 = 1.12;
const DRAGGING: f32 = 1.3;
const EPS: f32 = 0.01;
/// The container the fluid slider fills, narrow and wide.
const NARROW: f32 = 260.0;
const WIDE: f32 = 520.0;

#[derive(Clone, PartialEq)]
pub(super) struct Scene {
    pub start: f64,
    pub step: Option<f64>,
    pub max: f64,
    pub fluid: bool,
}

impl Default for Scene {
    fn default() -> Self {
        Scene {
            start: 50.0,
            step: None,
            max: 100.0,
            fluid: false,
        }
    }
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let start = self.start;
        let mut value = use_state(move || start);
        let mut wide = use_state(|| false);
        let mut line = slider(value(), move |next| value.set(next)).max(self.max);
        if let Some(step) = self.step {
            line = line.step(step);
        }
        if self.fluid {
            line = line.fill();
        }
        let container = if wide() { WIDE } else { NARROW };
        WindowHost::new(
            rect()
                .expanded()
                .tw(&tokens, &[bg(Paint::Bg), p(S4), flex_col(), gap(S4)])
                .on_global_key_down(move |event: Event<KeyboardEventData>| {
                    if event.key == Key::Character("w".into()) {
                        let next = !*wide.peek();
                        wide.set(next);
                    }
                })
                .child(
                    rect()
                        .width(if self.fluid {
                            Size::px(container)
                        } else {
                            Size::auto()
                        })
                        .child(line),
                )
                .child(
                    rect()
                        .tw(&tokens, &[w(Dim::Field)])
                        .child(BODY.label(&tokens, format!("v={:.1}", value()))),
                ),
        )
    }
}

struct Look {
    area: Area,
    track: Area,
    fill: Area,
    knob: Area,
}

fn look(frame: &Frame) -> Look {
    let area = |name: &str| {
        frame
            .nth(name, 0)
            .area
            .unwrap_or_else(|| panic!("{name} has no area"))
    };
    Look {
        area: area("SliderBox"),
        track: area("SliderTrack"),
        fill: area("SliderFill"),
        knob: area("SliderKnob"),
    }
}

fn centre_x(area: Area) -> f32 {
    area.min_x() + area.width() / 2.0
}

fn centre_y(area: Area) -> f32 {
    area.min_y() + area.height() / 2.0
}

/// The value the app holds, as the label under the slider shows it.
fn value_of(frame: &Frame) -> f64 {
    frame
        .texts()
        .iter()
        .find_map(|look| {
            look.text
                .strip_prefix("v=")
                .and_then(|rest| rest.parse::<f64>().ok())
        })
        .expect("the value label is on screen")
}

/// What must hold on every frame of every motion of the slider.
fn every_frame(frames: &[Frame], base_thumb: f32, findings: &mut Findings) {
    for (at, frame) in frames.iter().enumerate() {
        let now = look(frame);
        // The track fills the slider's box, its fill and knob are on the track's centre line.
        findings.check(
            (now.track.min_x() - now.area.min_x()).abs() <= 0.5
                && (now.track.width() - now.area.width()).abs() <= 0.5,
            || {
                format!(
                    "frame {at}: the track {:?} is not the slider's box {:?}",
                    now.track, now.area
                )
            },
        );
        let line = centre_y(now.track);
        findings.check((centre_y(now.fill) - line).abs() <= EPS, || {
            format!(
                "frame {at}: the fill's centre is {:.2}, the track's {line:.2}",
                centre_y(now.fill)
            )
        });
        findings.check((centre_y(now.knob) - line).abs() <= 0.5 + EPS, || {
            format!(
                "frame {at}: the knob's centre is {:.2}, the track's {line:.2}",
                centre_y(now.knob)
            )
        });
        // The knob is round, between its resting size and the size it grows to.
        findings.check((now.knob.width() - now.knob.height()).abs() <= EPS, || {
            format!("frame {at}: the knob is not round: {:?}", now.knob)
        });
        findings.check(
            now.knob.width() >= base_thumb - EPS && now.knob.width() <= base_thumb * DRAGGING + EPS,
            || {
                format!(
                    "frame {at}: the knob is {:.2} wide, between {base_thumb:.2} and {:.2}",
                    now.knob.width(),
                    base_thumb * DRAGGING
                )
            },
        );
        // The knob's centre is on the travel: between the centres of the knob at either end.
        let (lo, hi) = (
            now.track.min_x() + base_thumb / 2.0,
            now.track.max_x() - base_thumb / 2.0,
        );
        findings.check(
            centre_x(now.knob) >= lo - 0.5 - EPS && centre_x(now.knob) <= hi + 0.5 + EPS,
            || {
                format!(
                    "frame {at}: the knob's centre {:.2} is off the travel {lo:.2}..{hi:.2}",
                    centre_x(now.knob)
                )
            },
        );
        // The knob stays inside the slider's box but for its own growth.
        let grown = (now.knob.width() - base_thumb) / 2.0 + 0.5;
        findings.check(
            now.knob.min_x() >= now.area.min_x() - grown - EPS
                && now.knob.max_x() <= now.area.max_x() + grown + EPS,
            || {
                format!(
                    "frame {at}: the knob {:?} left the slider's box {:?}",
                    now.knob, now.area
                )
            },
        );
        // The fill ends at the knob's centre (the knob is placed on whole pixels).
        findings.check(
            (now.fill.max_x() - centre_x(now.knob)).abs() <= 0.5 + EPS,
            || {
                format!(
                    "frame {at}: the fill ends at {:.2}, the knob's centre is {:.2}",
                    now.fill.max_x(),
                    centre_x(now.knob)
                )
            },
        );
    }
}

/// Nothing of the window moves but the parts of the slider and the readout.
fn nothing_else_moves(frames: &[Frame], findings: &mut Findings) {
    findings.extend(
        unmoved(frames, EPS, |path| {
            // The ring's wrapper under the slider's box follows the knob, as the knob does.
            path.contains("SliderBox#0/column#0")
                || path.contains("SliderKnob")
                || path.contains("SliderFill")
                || path.contains("SliderTrack")
                || path.contains("Label")
        })
        .into_iter()
        .map(Err),
    );
}

fn thumb_of(rig: &Rig) -> f32 {
    look(rig.now()).knob.width()
}

/// The pointer position, in logical pixels, that puts the knob's centre at `percent` of the travel.
fn x_for(rig: &Rig, percent: f64, max: f64) -> f32 {
    let now = look(rig.now());
    let thumb = thumb_of(rig).min(now.track.height().max(16.0)).max(16.0);
    let (lo, hi) = (
        now.track.min_x() + thumb / 2.0,
        now.track.max_x() - thumb / 2.0,
    );
    lo + (hi - lo) * (percent / max) as f32
}

fn knob_point(rig: &Rig) -> (f32, f32) {
    let now = look(rig.now());
    (centre_x(now.knob), centre_y(now.knob))
}

#[test]
fn a_drag_carries_the_knob_with_the_pointer_and_keeps_it_on_the_line() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene::default());
        let base = thumb_of(&rig);
        let (x, y) = knob_point(&rig);
        rig.press_at(x, y);
        // To 80 %, past the right end, past the left end, back to 30 %: a move and a frame each.
        let targets = [
            x_for(&rig, 80.0, 100.0),
            look(rig.now()).track.max_x() + 80.0,
            look(rig.now()).track.min_x() - 200.0,
            x_for(&rig, 30.0, 100.0),
        ];
        let mut expected = Vec::new();
        for target in targets {
            // In ten steps, as a real pointer moves.
            let from = rig.frames.len();
            let _ = from;
            for part in 1..=10 {
                let last = knob_point(&rig).0;
                let at = last + (target - last) * (part as f32 / 10.0);
                rig.move_to(at, y);
                rig.step();
                let now = look(rig.now());
                let (lo, hi) = (
                    now.track.min_x() + base / 2.0,
                    now.track.max_x() - base / 2.0,
                );
                expected.push((at.clamp(lo, hi), centre_x(now.knob), part));
            }
        }
        rig.release_at(x_for(&rig, 30.0, 100.0), y);
        rig.steps(20);
        every_frame(&rig.frames, base, findings);
        nothing_else_moves(&rig.frames, findings);
        // After each pointer move and a frame the knob is under the pointer, clamped to the travel.
        for (want, got, part) in expected {
            findings.check((want - got).abs() <= 1.5, || {
                format!("the knob is at {got:.2}, the pointer (clamped to the travel) at {want:.2} (move {part})")
            });
        }
        // At rest the knob and the value agree, and the knob has shrunk back.
        let end = look(rig.now());
        findings.check((end.knob.width() - base).abs() <= EPS, || {
            format!("the knob did not shrink back: {:.2}", end.knob.width())
        });
        let value = value_of(rig.now());
        findings.check((value - 30.0).abs() <= 3.0, || {
            format!("the drag ended at {value}, near 30 was asked")
        });
    });
}

/// Whether every step of `values` goes towards the target of its frame, never past it and never
/// away: a motion that always aims at where it is going, from where it is.
fn steps_towards(what: &str, values: &[f32], targets: &[f32]) -> Result<(), String> {
    for at in 1..values.len() {
        let (before, now, target) = (values[at - 1], values[at], targets[at]);
        let (lo, hi) = (before.min(target), before.max(target));
        if now < lo - EPS || now > hi + EPS {
            return Err(format!(
                "{what}: frame {at} went from {before:.3} to {now:.3} with the target at {target:.3}: {}",
                show(values)
            ));
        }
    }
    Ok(())
}

#[test]
fn the_knob_follows_one_curve_when_the_pointer_comes_and_goes_every_frame() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene::default());
        let base = thumb_of(&rig);
        let (x, y) = knob_point(&rig);
        let away = (x + 400.0, y + 120.0);
        let mut targets = vec![base];
        // In, out, in, out on every frame; then in for a while; then a press, and out.
        for step in 0..16 {
            if step % 2 == 0 {
                rig.hover_at(x, y);
                targets.push(base * HOVER);
            } else {
                rig.hover_at(away.0, away.1);
                targets.push(base);
            }
            rig.step();
            targets.push(*targets.last().unwrap());
        }
        rig.hover_at(x, y);
        rig.steps(12);
        every_frame(&rig.frames, base, findings);
        let sizes: Vec<f32> = rig.frames.iter().map(|f| look(f).knob.width()).collect();
        // The knob only ever grows to the hover size or back: never past it, never a jump to a
        // size it was not on its way to.
        findings.extend([within("knob size", &sizes, base, base * HOVER, EPS)]);
        findings.extend([no_jump("knob size", &sizes, (base * HOVER - base) * 1.01)]);
        let _ = targets;
        findings.extend([ends_at("knob size", &sizes, base * HOVER, EPS)]);
    });
}

#[test]
fn a_press_grows_the_knob_to_the_held_size_and_a_release_gives_it_back_from_where_it_is() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene::default());
        let base = thumb_of(&rig);
        let (x, y) = knob_point(&rig);
        rig.hover_at(x, y);
        rig.steps(12);
        rig.press_at(x, y);
        rig.steps(3);
        // Let go half way up.
        rig.release_at(x, y);
        rig.steps(16);
        every_frame(&rig.frames, base, findings);
        let sizes: Vec<f32> = rig.frames.iter().map(|f| look(f).knob.width()).collect();
        findings.extend([within("knob size", &sizes, base, base * 1.2 + EPS, EPS)]);
        findings.extend([no_jump("knob size", &sizes, (base * 1.2 - base) * 1.01)]);
        // The knob does not leave its place when it grows: its centre stays within a pixel.
        let centres: Vec<f32> = rig.frames.iter().map(|f| centre_x(look(f).knob)).collect();
        findings.extend([constant(
            "knob centre x while it grows and shrinks",
            &centres,
            1.0,
        )]);
        let _ = steps_towards;
    });
}

#[test]
fn keys_and_the_pointer_interleaved_leave_the_knob_where_the_value_is() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(
            env,
            WINDOW,
            Scene {
                step: Some(5.0),
                ..Scene::default()
            },
        );
        let base = thumb_of(&rig);
        rig.key(Key::Named(NamedKey::Tab));
        let track = look(rig.now()).track;
        let at = |rig: &Rig, percent: f64| x_for(rig, percent, 100.0);
        // Right, Right, a click at 20 %, Left, End, Home, a click at 90 %, Page Down.
        rig.key(Key::Named(NamedKey::ArrowRight));
        rig.step();
        rig.key(Key::Named(NamedKey::ArrowRight));
        rig.step();
        let (x20, y) = (at(&rig, 20.0), centre_y(track));
        rig.click_at(x20, y);
        rig.step();
        rig.key(Key::Named(NamedKey::ArrowLeft));
        rig.step();
        rig.key(Key::Named(NamedKey::End));
        rig.step();
        rig.key(Key::Named(NamedKey::Home));
        rig.step();
        let x90 = at(&rig, 90.0);
        rig.click_at(x90, y);
        rig.step();
        rig.key(Key::Named(NamedKey::PageDown));
        rig.steps(20);
        every_frame(&rig.frames, base, findings);
        nothing_else_moves(&rig.frames, findings);
        let value = value_of(rig.now());
        findings.check((value - 80.0).abs() <= 5.1, || {
            format!("the value ended at {value}, near 80 was asked")
        });
        // The knob agrees with the value at rest.
        let now = look(rig.now());
        let want = at(&rig, value);
        findings.check((centre_x(now.knob) - want).abs() <= 1.0, || {
            format!(
                "the knob is at {:.2}, the value {value} puts it at {want:.2}",
                centre_x(now.knob)
            )
        });
    });
}

#[test]
fn the_knob_sits_on_the_ends_of_the_travel_at_the_least_and_the_most() {
    Env::each(&Env::ALL, |env, findings| {
        for (start, max) in [
            (0.0, 100.0),
            (100.0, 100.0),
            (0.0, 150.0),
            (150.0, 150.0),
            (100.0, 150.0),
        ] {
            let rig = Rig::new(
                env,
                WINDOW,
                Scene {
                    start,
                    max,
                    ..Scene::default()
                },
            );
            let base = thumb_of(&rig);
            every_frame(&rig.frames, base, findings);
            let now = look(rig.now());
            let want = if start == 0.0 {
                now.track.min_x() + base / 2.0
            } else {
                now.track.min_x() + base / 2.0 + (now.track.width() - base) * (start / max) as f32
            };
            findings.check((centre_x(now.knob) - want).abs() <= 0.5 + EPS, || {
                format!(
                    "value {start} of {max}: the knob is at {:.2}, expected {want:.2}",
                    centre_x(now.knob)
                )
            });
        }
    });
}

#[test]
fn a_fluid_slider_keeps_its_knob_on_its_track_while_its_container_resizes() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(
            env,
            WINDOW,
            Scene {
                fluid: true,
                start: 70.0,
                ..Scene::default()
            },
        );
        let base = thumb_of(&rig);
        // Wide, narrow, wide on consecutive frames, then settle.
        for _ in 0..4 {
            rig.key_char('w');
            rig.step();
        }
        rig.steps(20);
        every_frame(&rig.frames, base, findings);
        let widths: Vec<f32> = rig.frames.iter().map(|f| look(f).track.width()).collect();
        findings.extend([within(
            "track width",
            &widths,
            NARROW - 32.0 - 1.0,
            WIDE - 32.0 + 1.0,
            40.0,
        )]);
        // At rest the knob is at 70 % of the travel of the final track.
        let now = look(rig.now());
        let (lo, hi) = (
            now.track.min_x() + base / 2.0,
            now.track.max_x() - base / 2.0,
        );
        let want = lo + (hi - lo) * 0.7;
        findings.check((centre_x(now.knob) - want).abs() <= 1.0, || {
            format!(
                "after the resize the knob is at {:.2}, 70 % of the travel is {want:.2}",
                centre_x(now.knob)
            )
        });
    });
}

#[test]
fn reduced_motion_resizes_the_knob_at_once() {
    Env::each(&Env::REDUCED, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene::default());
        let base = thumb_of(&rig);
        let (x, y) = knob_point(&rig);
        rig.hover_at(x, y);
        rig.steps(8);
        every_frame(&rig.frames, base, findings);
        let sizes: Vec<f32> = rig.frames.iter().map(|f| look(f).knob.width()).collect();
        for (at, size) in sizes.iter().enumerate() {
            findings.check((size - base).abs() <= EPS || (size - base * HOVER).abs() <= EPS, || {
                format!(
                    "frame {at}: reduced motion drew the knob at {size:.2}, between {base:.2} and {:.2}",
                    base * HOVER
                )
            });
        }
        findings.extend([ends_at("knob size", &sizes, base * HOVER, EPS)]);
    });
}
