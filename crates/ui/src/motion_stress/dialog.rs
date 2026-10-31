//! The alert dialog as it opens and closes: its card fades and scales in place along one curve, over
//! a window that does not move.

use freya::prelude::*;

use super::*;
use crate::group::Group;
use crate::rows::row;
use crate::shell::dialog::{ResponseKind, dialog};
use crate::shell::{OverlayHost, use_overlays};
use crate::style::*;
use crate::surface::use_tokens;

const WINDOW: (f32, f32) = (900.0, 600.0);
const PHONE: (f32, f32) = (390.0, 800.0);
const EPS: f32 = 0.005;
/// A dialog's presence motion, from `shell/panel.rs`.
const MOTION_MS: u64 = 120;
const TITLE: &str = "Remove this network?";

#[derive(PartialEq)]
struct Page;

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Bg), p(S4)])
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                if event.key == Key::Character("d".into()) {
                    overlays.show_dialog(
                        dialog(
                            TITLE,
                            "It will be forgotten and you will have to enter its password again.",
                        )
                        .response("Cancel", ResponseKind::Default, |_| {})
                        .response("Remove", ResponseKind::Destructive, |_| {})
                        .default_action("cancel"),
                    );
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

/// The dialog's title: where it is and its folded opacity; `None` while it is not mounted.
fn title(frame: &Frame) -> Option<(Area, f32)> {
    let look = frame.texts().into_iter().find(|look| look.text == TITLE)?;
    Some((look.area, frame.opacity(look.id)))
}

fn opacities(frames: &[Frame]) -> Vec<f32> {
    frames
        .iter()
        .map(|frame| title(frame).map_or(0.0, |(_, o)| o))
        .collect()
}

fn every_frame(frames: &[Frame], findings: &mut Findings) {
    let mut seen: Option<Area> = None;
    for (at, frame) in frames.iter().enumerate() {
        let Some((area, opacity)) = title(frame) else {
            continue;
        };
        match seen {
            None => seen = Some(area),
            Some(first) => findings.check(area == first, || {
                format!("frame {at}: the dialog moved: {first:?} -> {area:?}")
            }),
        }
        findings.check((-EPS..=1.0 + EPS).contains(&opacity), || {
            format!("frame {at}: opacity {opacity:.3}")
        });
    }
    // The page behind does not move.
    findings.extend(
        unmoved(frames, 0.01, |path| {
            path.contains("Overlay") || path.contains("Dialog") || path.contains("Panel")
        })
        .into_iter()
        .map(Err),
    );
    findings.extend(elision_flips(frames, 0).into_iter().map(Err));
}

#[test]
fn a_dialog_fades_in_place_along_one_curve_and_ends_whole() {
    Env::matrix(&[WINDOW, PHONE], &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene);
        rig.key_char('d');
        rig.steps(16);
        every_frame(&rig.frames, findings);
        let o = opacities(&rig.frames);
        findings.extend([monotone_up("dialog opacity", &o, EPS)]);
        findings.extend([ends_at("dialog opacity", &o, 1.0, EPS)]);
        findings.check(o[0] <= EPS, || {
            format!(
                "the dialog was {:.2} opaque on the key frame: {}",
                o[0],
                show(&o)
            )
        });
        let (start, rest) = (start_index(&o, EPS), rest_index(&o, 0.01));
        let frames = rest + 1 - start;
        let wanted = MOTION_MS.div_ceil(STEP_MS) as usize;
        findings.check((wanted - 1..=wanted + 2).contains(&frames), || {
            format!(
                "the fade took {frames} frames, {MOTION_MS} ms is {wanted}: {}",
                show(&o)
            )
        });
        findings.extend(inside_window(&rig.frames, &[], 0.5).into_iter().map(Err));
        let (area, _) = title(rig.now()).expect("the dialog is open");
        findings.check(area.min_x() >= 0.0 && area.max_x() <= window.0, || {
            format!("the title {area:?} is outside the window")
        });
    });
}

#[test]
fn escape_fades_the_dialog_out_in_place_and_unmounts_it() {
    Env::matrix(&[WINDOW, PHONE], &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene);
        rig.key_char('d');
        rig.steps(16);
        let from = rig.frames.len();
        rig.key(Key::Named(NamedKey::Escape));
        rig.steps(16);
        every_frame(&rig.frames, findings);
        let o = opacities(&rig.frames[from - 1..]);
        findings.extend([monotone_down("dialog opacity", &o, EPS)]);
        findings.extend([ends_at("dialog opacity", &o, 0.0, EPS)]);
        findings.check(title(rig.now()).is_none(), || {
            "the dialog is still mounted after it closed".to_owned()
        });
    });
}

#[test]
fn closing_a_dialog_half_open_goes_back_from_where_it_is() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        rig.key_char('d');
        rig.steps(3);
        let turn = rig.frames.len();
        let before = opacities(&rig.frames)[turn - 1];
        rig.key(Key::Named(NamedKey::Escape));
        rig.steps(16);
        every_frame(&rig.frames, findings);
        let o = opacities(&rig.frames);
        findings.extend([monotone_down(
            "dialog opacity after the turn",
            &o[turn - 1..],
            EPS,
        )]);
        findings.check(o[turn] <= before + EPS, || {
            format!(
                "the dialog leapt from {before:.2} to {:.2} when closed",
                o[turn]
            )
        });
        findings.extend([ends_at("dialog opacity", &o, 0.0, EPS)]);
    });
}

#[test]
fn opening_and_closing_on_every_frame_ends_closed_and_never_flashes() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        for round in 0..6 {
            if round % 2 == 0 {
                rig.key_char('d');
            } else {
                rig.key(Key::Named(NamedKey::Escape));
            }
            rig.step();
        }
        rig.steps(20);
        every_frame(&rig.frames, findings);
        let o = opacities(&rig.frames);
        findings.extend([no_jump("dialog opacity", &o, 0.75)]);
        findings.extend([ends_at("dialog opacity", &o, 0.0, EPS)]);
    });
}

#[test]
fn reduced_motion_shows_and_hides_the_dialog_at_once() {
    Env::each(&Env::REDUCED, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene);
        rig.key_char('d');
        rig.steps(8);
        rig.key(Key::Named(NamedKey::Escape));
        rig.steps(8);
        every_frame(&rig.frames, findings);
        for (at, o) in opacities(&rig.frames).iter().enumerate() {
            findings.check(*o <= EPS || *o >= 1.0 - EPS, || {
                format!("frame {at}: reduced motion drew the dialog {o:.2} opaque")
            });
        }
    });
}
