//! The toast as it arrives: it fades in and rises into its place along one curve, inside the window,
//! over a page that does not move.

use freya::prelude::*;

use super::*;
use crate::group::Group;
use crate::rows::row;
use crate::shell::{OverlayHost, Toast, use_overlays};
use crate::style::*;
use crate::surface::use_tokens;

const WINDOW: (f32, f32) = (900.0, 600.0);
const PHONE: (f32, f32) = (390.0, 800.0);
const EPS: f32 = 0.01;
/// How a toast arrives, from `shell/overlays.rs`: it fades and rises `S2` in this long.
const MOTION_MS: u64 = 140;

#[derive(PartialEq)]
struct Page;

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let mut count = use_state(|| 0_u32);
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Bg), p(S4)])
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                if event.key == Key::Character("t".into()) {
                    let next = *count.peek() + 1;
                    count.set(next);
                    overlays.show_toast(Toast::new(format!("Saved {next}")).action("Undo", |_| {}));
                }
            })
            .child(Group::titled("Page").rows(vec![
                row("Before").value("A value").into(),
                row("After").value("Another").into(),
            ]))
    }
}

#[derive(Clone, PartialEq)]
struct Scene;

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        OverlayHost::new(Page)
    }
}

/// The toast's first text: where it is and how visible. `None` while there is no toast.
struct Seen {
    y: f32,
    x: f32,
    opacity: f32,
}

fn toast(frame: &Frame, text: &str) -> Option<Seen> {
    let look = frame.texts().into_iter().find(|look| look.text == text)?;
    Some(Seen {
        y: look.area.min_y(),
        x: look.area.min_x(),
        opacity: frame.opacity(look.id),
    })
}

fn toasts_on_screen(frame: &Frame) -> usize {
    frame.all("Toast").len()
}

fn every_frame(frames: &[Frame], findings: &mut Findings) {
    // The page behind never moves: the toast floats over it.
    findings.extend(
        unmoved(frames, EPS, |path| {
            path.contains("Toast") || path.contains("Overlay") || path.contains("Banner")
        })
        .into_iter()
        .map(Err),
    );
    for (at, frame) in frames.iter().enumerate() {
        findings.check(toasts_on_screen(frame) <= 1, || {
            format!("frame {at}: {} toasts at once", toasts_on_screen(frame))
        });
    }
    findings.extend(inside_window(frames, &["Toast"], 0.5).into_iter().map(Err));
}

#[test]
fn a_toast_fades_in_and_rises_into_place_along_one_curve() {
    Env::matrix(&[WINDOW, PHONE], &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene);
        rig.key_char('t');
        rig.steps(24);
        every_frame(&rig.frames, findings);
        let seen: Vec<Option<Seen>> = rig.frames.iter().map(|f| toast(f, "Saved 1")).collect();
        let first = seen
            .iter()
            .position(Option::is_some)
            .expect("the toast appeared");
        let ys: Vec<f32> = seen.iter().flatten().map(|s| s.y).collect();
        let os: Vec<f32> = seen.iter().flatten().map(|s| s.opacity).collect();
        findings.extend([monotone_down("toast y (it rises)", &ys, EPS)]);
        findings.extend([monotone_up("toast opacity", &os, EPS)]);
        findings.extend([ends_at("toast opacity", &os, 1.0, EPS)]);
        // It rises by S2 (8 px) and not a pixel more, from below its place to its place.
        let rise = ys[0] - ys[ys.len() - 1];
        findings.check(rise <= 8.0 + EPS && rise >= 0.0, || {
            format!("the toast rose {rise:.2} px, S2 is 8: {}", show(&ys))
        });
        // It is not drawn whole before it has begun to settle.
        findings.check(os[0] < 0.6, || {
            format!(
                "the toast's first frame is already {:.2} opaque: {}",
                os[0],
                show(&os)
            )
        });
        // It lands where it stays, to the hundredth of a pixel, and in time.
        let rest = rest_index(&ys, EPS);
        let wanted = MOTION_MS.div_ceil(STEP_MS) as usize + 1;
        findings.check(rest <= wanted + 2, || {
            format!(
                "the toast took {rest} frames to land, {wanted} expected: {}",
                show(&ys)
            )
        });
        let _ = first;
        let xs: Vec<f32> = seen.iter().flatten().map(|s| s.x).collect();
        findings.extend([constant("toast x", &xs, EPS)]);
    });
}

#[test]
fn toasts_queued_on_every_frame_show_one_at_a_time_in_order_and_move_nothing_else() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        for _ in 0..5 {
            rig.key_char('t');
            rig.step();
        }
        rig.steps(30);
        every_frame(&rig.frames, findings);
        // Only the first of the five is on screen, and it stays: the rest wait behind it.
        for (at, frame) in rig.frames.iter().enumerate() {
            for later in 2..=5 {
                findings.check(toast(frame, &format!("Saved {later}")).is_none(), || {
                    format!("frame {at}: toast {later} showed while the first was up")
                });
            }
        }
        let seen: Vec<f32> = rig
            .frames
            .iter()
            .filter_map(|f| toast(f, "Saved 1"))
            .map(|s| s.y)
            .collect();
        findings.extend([monotone_down("the first toast's y", &seen, EPS)]);
    });
}

#[test]
fn the_pointer_over_a_toast_while_it_arrives_does_not_move_it() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        rig.key_char('t');
        rig.steps(2);
        let spot = rig
            .now()
            .texts()
            .into_iter()
            .find(|look| look.text == "Saved 1")
            .expect("the toast is up")
            .area;
        for step in 0..14 {
            if step % 2 == 0 {
                rig.hover_at(spot.min_x() + 4.0, spot.min_y() + 4.0);
            } else {
                rig.hover_at(10.0, 10.0);
            }
            rig.step();
        }
        rig.steps(10);
        every_frame(&rig.frames, findings);
        let ys: Vec<f32> = rig
            .frames
            .iter()
            .filter_map(|f| toast(f, "Saved 1"))
            .map(|s| s.y)
            .collect();
        findings.extend([monotone_down(
            "toast y while the pointer comes and goes",
            &ys,
            EPS,
        )]);
    });
}

#[test]
fn reduced_motion_puts_the_toast_up_whole_at_once() {
    Env::each(&Env::REDUCED, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        rig.key_char('t');
        rig.steps(10);
        every_frame(&rig.frames, findings);
        // A frame in which the toast is mounted and still invisible is not drawn; every frame in
        // which it is drawn at all is the whole toast at its place.
        let seen: Vec<Seen> = rig
            .frames
            .iter()
            .filter_map(|f| toast(f, "Saved 1"))
            .filter(|s| s.opacity > EPS)
            .collect();
        findings.check(!seen.is_empty(), || "the toast never showed".to_owned());
        for (at, s) in seen.iter().enumerate() {
            findings.check(s.opacity >= 1.0 - EPS, || {
                format!(
                    "toast frame {at}: reduced motion drew it {:.2} opaque",
                    s.opacity
                )
            });
        }
        let ys: Vec<f32> = seen.iter().map(|s| s.y).collect();
        findings.extend([constant("toast y under reduced motion", &ys, EPS)]);
    });
}
