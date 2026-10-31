//! The expander row opening and closing: the rows below move by exactly the height of what opens,
//! along one curve, and everything above and around stays.

use freya::prelude::*;

use super::*;
use crate::group::Group;
use crate::motion::MOVE_MS;
use crate::rows::{expander_row, row, switch_row};
use crate::style::*;
use crate::surface::use_tokens;

const DESKTOP: (f32, f32) = (900.0, 700.0);
const PHONE: (f32, f32) = (390.0, 800.0);
const EPS: f32 = 0.01;
const HEADER: &str = "Advanced";
const BELOW: &str = "After";
const ABOVE: &str = "Before";

#[derive(Clone, PartialEq)]
struct Scene {
    open: bool,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        WindowHost::new(
            rect()
                .expanded()
                .tw(&tokens, &[bg(Paint::Bg), p(S4)])
                .child(Group::titled("Network").rows(vec![
            row(ABOVE).value("A value").into(),
            expander_row(
                HEADER,
                vec![
                    switch_row("Connect automatically", true, |_| {}).into(),
                    row("Hardware address").value("02:1A:2B:3C:4D:5E").into(),
                    row("Metric of the default route on this adapter").value("100").into(),
                ],
            )
            .subtitle("Three rows")
            .expanded(self.open)
            .into(),
            row(BELOW).value("Another").into(),
        ])),
        )
    }
}

fn y_of(frame: &Frame, text: &str) -> f32 {
    frame
        .texts()
        .iter()
        .find(|look| look.text == text)
        .unwrap_or_else(|| panic!("no text {text:?}"))
        .area
        .min_y()
}

fn click_header(rig: &mut Rig) {
    let area = rig
        .now()
        .texts()
        .into_iter()
        .find(|look| look.text == HEADER)
        .expect("the header")
        .area;
    rig.click_at(
        area.min_x() + area.width() / 2.0,
        area.min_y() + area.height() / 2.0,
    );
}

fn every_frame(frames: &[Frame], findings: &mut Findings) {
    let above: Vec<f32> = frames.iter().map(|f| y_of(f, ABOVE)).collect();
    let header: Vec<f32> = frames.iter().map(|f| y_of(f, HEADER)).collect();
    findings.extend([constant("the row above the expander", &above, EPS)]);
    findings.extend([constant("the expander's header", &header, EPS)]);
    // Texts keep their elision: opening rows never cuts a label that was whole.
    findings.extend(elision_flips(frames, 0).into_iter().map(Err));
}

/// How far the row below sits from where it is when the expander is closed.
fn shift(frame: &Frame, closed_y: f32) -> f32 {
    y_of(frame, BELOW) - closed_y
}

#[test]
fn opening_pushes_the_rows_below_down_by_the_height_of_the_rows_along_one_curve() {
    Env::matrix(&[DESKTOP, PHONE], &Env::ALL, |env, window, findings| {
        let closed = Rig::new(env, window, Scene { open: false });
        let closed_y = y_of(closed.now(), BELOW);
        let open = Rig::new(env, window, Scene { open: true });
        let height = y_of(open.now(), BELOW) - closed_y;
        let mut rig = Rig::new(env, window, Scene { open: false });
        click_header(&mut rig);
        rig.steps(24);
        every_frame(&rig.frames, findings);
        let ys: Vec<f32> = rig.frames.iter().map(|f| shift(f, closed_y)).collect();
        findings.extend([monotone_up("the row below", &ys, EPS)]);
        findings.extend([within("the row below", &ys, 0.0, height, EPS)]);
        findings.extend([ends_at("the row below", &ys, height, EPS)]);
        // It begins at the closed place and ends at the place of an expander that mounted open.
        findings.check(ys[0].abs() <= EPS, || {
            format!("the row below began {:.2} off its place", ys[0])
        });
        // Over MOVE_MS at 16 ms a frame.
        let (start, rest) = (start_index(&ys, EPS), rest_index(&ys, EPS));
        let frames = rest + 1 - start;
        let wanted = MOVE_MS.div_ceil(STEP_MS) as usize;
        findings.check((wanted - 1..=wanted + 2).contains(&frames), || {
            format!(
                "the open took {frames} frames, {MOVE_MS} ms is {wanted}: {}",
                show(&ys)
            )
        });
        findings.check(start <= 5, || {
            format!("the rows began to move on frame {start}")
        });
        // A frame never moves the rows by more than the curve's biggest step (half of it).
        findings.extend([no_jump("the row below", &ys, height * 0.6)]);
    });
}

#[test]
fn closing_pulls_them_back_up_along_the_same_curve() {
    Env::matrix(&[DESKTOP, PHONE], &Env::ALL, |env, window, findings| {
        let closed = Rig::new(env, window, Scene { open: false });
        let closed_y = y_of(closed.now(), BELOW);
        let mut rig = Rig::new(env, window, Scene { open: true });
        let height = y_of(rig.now(), BELOW) - closed_y;
        click_header(&mut rig);
        rig.steps(24);
        every_frame(&rig.frames, findings);
        let ys: Vec<f32> = rig.frames.iter().map(|f| shift(f, closed_y)).collect();
        findings.extend([monotone_down("the row below", &ys, EPS)]);
        findings.extend([within("the row below", &ys, 0.0, height, EPS)]);
        findings.extend([ends_at("the row below", &ys, 0.0, EPS)]);
        findings.extend([no_jump("the row below", &ys, height * 0.6)]);
    });
}

#[test]
fn toggling_on_every_frame_never_overshoots_and_ends_on_the_last_state() {
    Env::matrix(&[DESKTOP, PHONE], &Env::ALL, |env, window, findings| {
        let closed = Rig::new(env, window, Scene { open: false });
        let closed_y = y_of(closed.now(), BELOW);
        let height = y_of(Rig::new(env, window, Scene { open: true }).now(), BELOW) - closed_y;
        for gap in [0usize, 1, 2, 4] {
            let mut rig = Rig::new(env, window, Scene { open: false });
            // Seven toggles: it ends open.
            for _ in 0..7 {
                click_header(&mut rig);
                rig.steps(gap);
            }
            rig.steps(30);
            every_frame(&rig.frames, findings);
            let ys: Vec<f32> = rig.frames.iter().map(|f| shift(f, closed_y)).collect();
            findings.extend([within(
                &format!("the row below ({gap} frames apart)"),
                &ys,
                0.0,
                height,
                EPS,
            )]);
            findings.extend([no_jump(
                &format!("the row below ({gap} frames apart)"),
                &ys,
                height * 0.6,
            )]);
            findings.extend([ends_at(
                &format!("the row below ({gap} frames apart)"),
                &ys,
                height,
                EPS,
            )]);
            let calm = Rig::new(env, window, Scene { open: true });
            let diff = signature_diff(rig.now(), calm.now(), 4);
            findings.check(diff.is_empty(), || {
                format!(
                    "{gap} frames apart: the settled frame differs from a calm open one: {diff:?}"
                )
            });
        }
    });
}

#[test]
fn turning_a_close_around_half_way_continues_from_the_height_on_screen() {
    Env::matrix(&[DESKTOP, PHONE], &Env::ALL, |env, window, findings| {
        let closed = Rig::new(env, window, Scene { open: false });
        let closed_y = y_of(closed.now(), BELOW);
        let mut rig = Rig::new(env, window, Scene { open: true });
        let height = y_of(rig.now(), BELOW) - closed_y;
        click_header(&mut rig);
        let mut guard = 0;
        while shift(rig.now(), closed_y) > height * 0.5 {
            rig.step();
            guard += 1;
            assert!(guard < 40, "the expander never got half closed");
        }
        let turn = rig.frames.len();
        let before = shift(rig.now(), closed_y);
        click_header(&mut rig);
        rig.steps(24);
        every_frame(&rig.frames, findings);
        let ys: Vec<f32> = rig.frames.iter().map(|f| shift(f, closed_y)).collect();
        findings.extend([monotone_up(
            "the row below after the turn",
            &ys[turn - 1..],
            EPS,
        )]);
        findings.check(
            ys[turn] >= before - height * 0.05 && ys[turn] <= before + height * 0.6,
            || {
                format!(
                    "the rows leapt from {before:.1} to {:.1} when turned",
                    ys[turn]
                )
            },
        );
        findings.extend([ends_at("the row below", &ys, height, EPS)]);
    });
}

#[test]
fn the_keyboard_opens_and_closes_it_like_the_pointer() {
    Env::each(&Env::ALL, |env, findings| {
        let closed = Rig::new(env, DESKTOP, Scene { open: false });
        let closed_y = y_of(closed.now(), BELOW);
        let height = y_of(Rig::new(env, DESKTOP, Scene { open: true }).now(), BELOW) - closed_y;
        let mut rig = Rig::new(env, DESKTOP, Scene { open: false });
        rig.key(Key::Named(NamedKey::Tab));
        rig.key(Key::Named(NamedKey::Tab));
        rig.key(Key::Named(NamedKey::Enter));
        rig.steps(2);
        rig.key(Key::Named(NamedKey::ArrowLeft));
        rig.steps(2);
        rig.key(Key::Named(NamedKey::ArrowRight));
        rig.steps(24);
        every_frame(&rig.frames, findings);
        let ys: Vec<f32> = rig.frames.iter().map(|f| shift(f, closed_y)).collect();
        findings.extend([within("the row below", &ys, 0.0, height, EPS)]);
        findings.extend([ends_at("the row below", &ys, height, EPS)]);
    });
}

#[test]
fn reduced_motion_opens_it_at_once() {
    Env::each(&Env::REDUCED, |env, findings| {
        let closed = Rig::new(env, DESKTOP, Scene { open: false });
        let closed_y = y_of(closed.now(), BELOW);
        let height = y_of(Rig::new(env, DESKTOP, Scene { open: true }).now(), BELOW) - closed_y;
        let mut rig = Rig::new(env, DESKTOP, Scene { open: false });
        click_header(&mut rig);
        rig.steps(10);
        every_frame(&rig.frames, findings);
        for (at, y) in rig.frames.iter().map(|f| shift(f, closed_y)).enumerate() {
            findings.check(y.abs() <= EPS || (y - height).abs() <= EPS, || {
                format!(
                    "frame {at}: reduced motion drew the row at {y:.2}, between 0 and {height:.2}"
                )
            });
        }
    });
}
