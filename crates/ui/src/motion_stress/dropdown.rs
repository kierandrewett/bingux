//! The dropdown's list as it opens and closes: one fade and scale of its card, in place, along one
//! curve, whatever is done to the window meanwhile.
//!
//! The list is a popover on a desktop window and a sheet on a phone (the sheet has its own frame
//! tests in `shell/sheet_motion_frame_tests.rs`); these are the popover.

use freya::prelude::*;

use super::*;
use crate::controls::dropdown;
use crate::group::Group;
use crate::rows::{row, switch_row};
use crate::style::*;
use crate::surface::use_tokens;

const WINDOW: (f32, f32) = (900.0, 640.0);
const EPS: f32 = 0.005;
/// The popover's presence motion, from `shell/panel.rs`.
const MOTION_MS: u64 = 90;
/// What a presence starts scaled to, from `motion.rs`.
const SCALE0: f32 = 0.96;

#[derive(Clone, PartialEq)]
struct Scene {
    /// Rows above the dropdown: a few put it by the window's bottom edge, many make the page scroll.
    fillers: usize,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut choice = use_state(|| "b".to_owned());
        let options: Vec<(String, String)> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|k| (k.to_string(), format!("Choice {k}")))
            .collect();
        let mut rows: Vec<Element> = Vec::new();
        for index in 0..self.fillers {
            rows.push(row(format!("Filler {index}")).value("A value").into());
        }
        rows.push(
            row("Pick one")
                .trailing(
                    dropdown(options, choice.read().clone(), move |next: String| {
                        choice.set(next)
                    })
                    .title("Pick one"),
                )
                .into(),
        );
        rows.push(switch_row("After the dropdown", true, |_| {}).into());
        WindowHost::new(
            rect()
                .expanded()
                .tw(&tokens, &[bg(Paint::Bg), p(S4)])
                .child(
                    ScrollView::new()
                        .width(Size::fill())
                        .height(Size::fill())
                        .child(Group::titled("Choices").rows(rows)),
                ),
        )
    }
}

/// One frame of the list: its rows' union box and the opacity of the first row (every box around
/// it folded in), `None` while the list is not mounted.
struct List {
    area: Area,
    opacity: f32,
    rows: usize,
}

fn list(frame: &Frame) -> Option<List> {
    let rows = frame.all("DropdownItem");
    let first = rows.first()?;
    let mut area = first.area?;
    for row in &rows {
        area = area.union(&row.area?);
    }
    Some(List {
        area,
        opacity: frame.opacity(first.id),
        rows: rows.len(),
    })
}

fn button(frame: &Frame) -> Area {
    frame.area("Dropdown")
}

/// What must hold on every frame of an open, a close or anything between.
fn every_frame(frames: &[Frame], findings: &mut Findings) {
    let mut seen: Option<Area> = None;
    for (at, frame) in frames.iter().enumerate() {
        let Some(now) = list(frame) else { continue };
        // A list that is on screen at all is where it will stay, in every frame: the motion is a
        // fade and a scale of the card, not a slide.
        match seen {
            None => seen = Some(now.area),
            Some(first) => findings.check(now.area == first, || {
                format!(
                    "frame {at}: the list moved or resized: {first:?} -> {:?}",
                    now.area
                )
            }),
        }
        findings.check((-EPS..=1.0 + EPS).contains(&now.opacity), || {
            format!("frame {at}: opacity {:.3}", now.opacity)
        });
    }
}

/// The list's opacity at each frame; 0 where it is not mounted.
fn opacities(frames: &[Frame]) -> Vec<f32> {
    frames
        .iter()
        .map(|frame| list(frame).map_or(0.0, |l| l.opacity))
        .collect()
}

fn open_list(rig: &mut Rig) {
    rig.click("Dropdown");
}

#[test]
fn opening_fades_the_list_in_place_along_one_curve_and_ends_whole() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene { fillers: 0 });
        let anchor = button(rig.now());
        open_list(&mut rig);
        rig.steps(16);
        every_frame(&rig.frames, findings);
        let o = opacities(&rig.frames);
        findings.extend([monotone_up("list opacity", &o, EPS)]);
        findings.extend([ends_at("list opacity", &o, 1.0, EPS)]);
        // Not a single frame shows the list at full strength before it has a place: it is either
        // absent or on its way.
        findings.check(o[0] <= EPS, || {
            format!(
                "the list was already {:.2} on the click frame: {}",
                o[0],
                show(&o)
            )
        });
        // The button the list belongs to does not move.
        let moved: Vec<String> = rig
            .frames
            .iter()
            .enumerate()
            .filter(|(_, frame)| button(frame) != anchor)
            .map(|(at, frame)| {
                format!(
                    "frame {at}: the button moved {anchor:?} -> {:?}",
                    button(frame)
                )
            })
            .collect();
        findings.extend(moved.into_iter().map(Err));
        // The list opens against its button: just below it, in reach.
        let end = list(rig.now()).expect("the list is open");
        findings.check(end.rows == 5, || {
            format!("the list shows {} rows, not 5", end.rows)
        });
        findings.check((end.area.min_y() - anchor.max_y()).abs() < 400.0, || {
            format!("the list at {:?} is not by its button {anchor:?}", end.area)
        });
        // The motion's length: the fade is whole within its time at 16 ms a frame, and is not
        // done early.
        let (start, rest) = (start_index(&o, EPS), rest_index(&o, 0.01));
        let frames = rest + 1 - start;
        let wanted = MOTION_MS.div_ceil(STEP_MS) as usize;
        findings.check((wanted - 1..=wanted + 2).contains(&frames), || {
            format!(
                "the fade took {frames} frames, {MOTION_MS} ms is {wanted}: {}",
                show(&o)
            )
        });
        let _ = SCALE0;
    });
}

#[test]
fn closing_fades_the_list_out_in_place_and_unmounts_it() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOWS_NONE, Scene { fillers: 0 });
        open_list(&mut rig);
        rig.steps(16);
        let from = rig.frames.len();
        rig.key(Key::Named(NamedKey::Escape));
        rig.steps(16);
        every_frame(&rig.frames, findings);
        let o = opacities(&rig.frames[from - 1..]);
        findings.extend([monotone_down("list opacity", &o, EPS)]);
        findings.extend([ends_at("list opacity", &o, 0.0, EPS)]);
        findings.check(list(rig.now()).is_none(), || {
            "the list is still mounted after it closed".to_owned()
        });
    });
}

const WINDOWS_NONE: (f32, f32) = WINDOW;

#[test]
fn closing_a_half_open_list_goes_back_from_where_the_fade_is() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene { fillers: 0 });
        open_list(&mut rig);
        rig.steps(3);
        let at_turn = rig.frames.len();
        let before = list(rig.now()).map_or(0.0, |l| l.opacity);
        rig.key(Key::Named(NamedKey::Escape));
        rig.steps(14);
        every_frame(&rig.frames, findings);
        let o = opacities(&rig.frames);
        // After the turn it only goes down, from where it was: it does not leap to full first.
        findings.extend([monotone_down(
            "list opacity after the turn",
            &o[at_turn - 1..],
            EPS,
        )]);
        findings.check(o[at_turn] <= before + EPS, || {
            format!(
                "the list leapt from {before:.2} to {:.2} when it was closed",
                o[at_turn]
            )
        });
        findings.extend([ends_at("list opacity", &o, 0.0, EPS)]);
    });
}

#[test]
fn opening_and_closing_on_every_frame_never_flashes_and_ends_closed() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene { fillers: 0 });
        for round in 0..6 {
            if round % 2 == 0 {
                open_list(&mut rig);
            } else {
                rig.key(Key::Named(NamedKey::Escape));
            }
            rig.step();
        }
        rig.steps(20);
        every_frame(&rig.frames, findings);
        let o = opacities(&rig.frames);
        // No frame steps by more than the fade does in a frame: the list is never flashed on.
        findings.extend([no_jump("list opacity", &o, 0.75)]);
        findings.extend([ends_at("list opacity", &o, 0.0, EPS)]);
    });
}

#[test]
fn a_list_open_over_a_scrolling_page_stays_by_its_button_or_closes() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene { fillers: 14 });
        // Bring the dropdown into view: it is below the filler rows.
        rig.wheel(450.0, 300.0, -600.0);
        rig.steps(30);
        let anchor = button(rig.now());
        findings.check(anchor.max_y() <= WINDOW.1 && anchor.min_y() >= 0.0, || {
            format!("the button is not on screen after scrolling to it: {anchor:?}")
        });
        let mut rig = rig;
        let start = rig.frames.len();
        open_list(&mut rig);
        rig.steps(8);
        // Now scroll the page behind the open list, in both directions, on every other frame.
        for step in 0..12 {
            rig.wheel(450.0, 300.0, if step % 2 == 0 { -90.0 } else { 140.0 });
            rig.step();
        }
        rig.steps(20);
        every_frame(&rig.frames[start..], findings);
        // On every frame the list is either gone or at the gap it opened at under its button.
        let mut gap: Option<f32> = None;
        for (at, frame) in rig.frames[start..].iter().enumerate() {
            let Some(now) = list(frame) else { continue };
            let here = now.area.min_y() - button(frame).max_y();
            match gap {
                None => gap = Some(here),
                Some(first) => findings.check((here - first).abs() <= 1.0, || {
                    format!("frame {at}: the list is {here:.1} px below its button, it opened {first:.1} below")
                }),
            }
        }
    });
}

#[test]
fn a_list_opened_by_the_bottom_edge_stays_inside_the_window_on_every_frame() {
    // Three rows above the dropdown in a short window: there is no room under its button.
    let short = (900.0, 330.0);
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, short, Scene { fillers: 3 });
        let button = button(rig.now());
        findings.check(button.max_y() <= short.1, || {
            format!("the button {button:?} is not in the window")
        });
        open_list(&mut rig);
        rig.steps(16);
        every_frame(&rig.frames, findings);
        for (at, frame) in rig.frames.iter().enumerate() {
            let Some(now) = list(frame) else { continue };
            findings.check(
                now.area.min_y() >= -0.5
                    && now.area.max_y() <= short.1 + 0.5
                    && now.area.min_x() >= -0.5
                    && now.area.max_x() <= short.0 + 0.5,
                || {
                    format!(
                        "frame {at}: the list {:?} leaves the {}x{} window",
                        now.area, short.0, short.1
                    )
                },
            );
        }
    });
}

#[test]
fn pointer_and_keys_interleaved_while_the_list_opens_choose_one_option_and_close_it() {
    Env::each(&Env::ALL, |env, findings| {
        let mut rig = Rig::new(env, WINDOW, Scene { fillers: 0 });
        open_list(&mut rig);
        rig.step();
        // Down twice, then hover the first row, then Enter on the keyboard's row.
        rig.key(Key::Named(NamedKey::ArrowDown));
        rig.key(Key::Named(NamedKey::ArrowDown));
        rig.step();
        if let Some(l) = list(rig.now()) {
            rig.hover_at(l.area.min_x() + 40.0, l.area.min_y() + 12.0);
        }
        rig.step();
        rig.key(Key::Named(NamedKey::Enter));
        rig.steps(20);
        every_frame(&rig.frames, findings);
        findings.check(list(rig.now()).is_none(), || {
            "the list stayed open after Enter".to_owned()
        });
        let o = opacities(&rig.frames);
        findings.extend([ends_at("list opacity", &o, 0.0, EPS)]);
    });
}
