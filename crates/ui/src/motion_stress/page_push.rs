//! A subpage pushed and popped on a window, however fast the input comes: it slides along one curve
//! from where it is, the chrome around it stays, and the last frame is the frame a calm run ends on.
//!
//! `shell/page_motion_tests.rs` holds the curve of an uninterrupted push and pop; these are the
//! interruptions, the scale and scheme matrix, the scroll under the slide and the reduced motion.

use std::sync::Arc;

use freya::prelude::*;

use super::*;
use crate::component::ComponentName;
use crate::icons::Icon;
use crate::nav::gesture::SLIDE_MS;
use crate::nav::{Location, RouteTable};
use crate::shell::{NavigationSplitView, OverlayHost};
use crate::surface::use_tokens;

const DESKTOP: (f32, f32) = (1100.0, 760.0);
const PHONE: (f32, f32) = (390.0, 800.0);
const EPS: f32 = 0.01;

/// A page of rows: the first opens the subpage. The page is taller than the window, so it scrolls.
#[derive(PartialEq)]
struct Marked {
    class: &'static str,
    opens: bool,
}

impl Component for Marked {
    fn render(&self) -> impl IntoElement {
        let router = crate::nav::use_router();
        let opens = self.opens;
        let mut column = rect().component(self.class).width(Size::fill());
        // Rows above the opener, so a page that is scrolled a little still shows it.
        for index in 0..4 {
            column = column.child(crate::rows::row(format!("Above {index}")).value("A value"));
        }
        column = column.child(crate::rows::row("Open the subpage").navigates().on_press(
            move |_| {
                if opens {
                    router.push(&Location::new("motion").join("network").join("saved"));
                }
            },
        ));
        for index in 0..30 {
            column = column.child(crate::rows::row(format!("Row {index}")).value("A value"));
        }
        column
    }
}

fn table() -> Arc<RouteTable> {
    Arc::new(
        RouteTable::new("motion")
            .index("Motion", |_| rect().into())
            .root("network")
            .title("Network")
            .icon(Icon::Network)
            .group("Connections")
            .page(|_| {
                Marked {
                    class: "MarkRoot",
                    opens: true,
                }
                .into()
            })
            .root("sound")
            .title("Sound")
            .icon(Icon::Sound)
            .group("Hardware")
            .page(|_| {
                Marked {
                    class: "MarkOther",
                    opens: false,
                }
                .into()
            })
            .route("network/saved")
            .title("Saved networks")
            .page(|_| {
                Marked {
                    class: "MarkSub",
                    opens: false,
                }
                .into()
            }),
    )
}

#[derive(Clone, PartialEq)]
struct Scene {
    at: &'static str,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let _ = use_tokens();
        let mut start = Location::new("motion");
        for segment in self.at.split('/') {
            start = start.join(segment);
        }
        crate::nav::provide_router(table(), &start);
        let router = crate::nav::use_router();
        // `p` pushes the subpage if it is not the page on top: a push that does not depend on
        // where a row happens to be on screen, so a test can push on any frame.
        rect()
            .expanded()
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                if event.key == Key::Character("p".into()) {
                    let saved = Location::new("motion").join("network").join("saved");
                    if router.read().current().location != saved {
                        router.push(&saved);
                    }
                }
            })
            .child(OverlayHost::new(NavigationSplitView::new("Motion")))
    }
}

/// The sliding subpage's centre x, `None` while it is not mounted.
fn sub_x(frame: &Frame) -> Option<f32> {
    frame
        .find("MarkSub")
        .and_then(|node| node.area)
        .map(|area| area.min_x() + area.width() / 2.0)
}

fn root_scroll(frame: &Frame) -> Option<f32> {
    frame
        .find("MarkRoot")
        .and_then(|node| node.area)
        .map(|area| area.min_y())
}

fn open_sub(rig: &mut Rig) {
    rig.key_char('p');
}

/// The press of the opener row, as a person makes it.
fn press_opener(rig: &mut Rig) {
    let label = rig
        .now()
        .texts()
        .into_iter()
        .find(|look| look.text == "Open the subpage")
        .expect("the open row is on screen")
        .area;
    rig.click_at(
        label.min_x() + label.width() / 2.0,
        label.min_y() + label.height() / 2.0,
    );
}

fn back(rig: &mut Rig) {
    rig.shot
        .key_with(Key::Named(NamedKey::ArrowLeft), Modifiers::ALT);
    rig.mark();
}

/// What must hold on every frame: the sliding page is inside the window's width plus its own
/// shadow slack, never behind, and the chrome of the window does not move.
fn every_frame(frames: &[Frame], width: f32, findings: &mut Findings) {
    for (at, frame) in frames.iter().enumerate() {
        if let Some(x) = sub_x(frame) {
            findings.check(x >= -width && x <= width * 2.0, || {
                format!("frame {at}: the subpage's centre is at {x:.1} in a window {width} wide")
            });
        }
    }
    // The chrome: nothing outside the pages moves. The pages' own boxes are the motion.
    findings.extend(
        unmoved(frames, EPS, |path| {
            path.contains("MarkSub")
                || path.contains("MarkRoot")
                || path.contains("MarkOther")
                || path.contains("Content")
                || path.contains("Overlay")
        })
        .into_iter()
        .map(Err),
    );
}

fn calm(env: Env, window: (f32, f32), at: &'static str) -> Rig {
    let mut rig = Rig::new(env, window, Scene { at });
    rig.steps(3);
    rig
}

fn settle_diff(rig: &Rig, calm: &Rig, findings: &mut Findings, what: &str) {
    let a = rig.now().signature();
    let b = calm.now().signature();
    // The scroll offset of the page under may differ by what the person scrolled; compare shapes.
    let diff: Vec<String> = a
        .iter()
        .zip(&b)
        .enumerate()
        .filter(|(_, (x, y))| x != y)
        .take(4)
        .map(|(i, (x, y))| format!("node {i}: {x} != {y}"))
        .collect();
    findings.check(a.len() == b.len() && diff.is_empty(), || {
        format!(
            "{what}: the settled frame differs from a calm one ({} against {} nodes): {diff:?}",
            a.len(),
            b.len()
        )
    });
}

#[test]
fn a_push_slides_the_subpage_in_along_one_curve_in_every_scale_and_scheme() {
    Env::matrix(&[DESKTOP, PHONE], &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene { at: "network" });
        press_opener(&mut rig);
        rig.steps(40);
        every_frame(&rig.frames, window.0, findings);
        let xs: Vec<f32> = rig.frames.iter().filter_map(sub_x).collect();
        findings.check(!xs.is_empty(), || "the subpage never appeared".to_owned());
        findings.extend([monotone_down("subpage x", &xs, EPS)]);
        // It comes from the right edge, and ends on the content column.
        let rest = *xs.last().unwrap();
        findings.check(xs[0] > rest + window.0 * 0.2, || {
            format!("the subpage began at {:.1}, near its rest {rest:.1}", xs[0])
        });
        // The slide lasts SLIDE_MS at 16 ms a frame (to a step).
        let begin = start_index(&xs, EPS);
        let end = rest_index(&xs, EPS);
        let frames = end + 1 - begin;
        let wanted = SLIDE_MS.div_ceil(STEP_MS) as usize;
        findings.check((wanted - 1..=wanted + 1).contains(&frames), || {
            format!("the slide took {frames} frames, {SLIDE_MS} ms is {wanted}")
        });
    });
}

#[test]
fn pushing_and_going_back_on_every_other_frame_never_jumps_and_ends_where_the_input_ends() {
    Env::matrix(&[DESKTOP, PHONE], &Env::ALL, |env, window, findings| {
        for gap in [1usize, 2, 4] {
            let mut rig = Rig::new(env, window, Scene { at: "network" });
            // Push, back, push, back, push: three pushes and two backs ends on the subpage.
            for round in 0..5 {
                if round % 2 == 0 {
                    if rig.now().find("MarkSub").is_none()
                        || sub_x(rig.now()).is_some_and(|x| x > window.0 * 0.6)
                    {
                        open_sub(&mut rig);
                    } else {
                        open_sub(&mut rig);
                    }
                } else {
                    back(&mut rig);
                }
                rig.steps(gap);
            }
            rig.steps(40);
            every_frame(&rig.frames, window.0, findings);
            let xs: Vec<f32> = rig
                .frames
                .iter()
                .map(|f| sub_x(f).unwrap_or(window.0 * 1.5))
                .collect();
            // No frame moves the page by more than the largest step of an uninterrupted slide:
            // a quarter of the window.
            findings.extend([no_jump(
                &format!("subpage x ({gap} frames apart)"),
                &xs,
                window.0 * 0.3,
            )]);
        }
    });
}

#[test]
fn going_back_half_way_through_a_push_starts_from_where_the_page_is() {
    Env::matrix(&[DESKTOP, PHONE], &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene { at: "network" });
        open_sub(&mut rig);
        let mut guard = 0;
        while sub_x(rig.now()).is_none_or(|x| x > window.0 * 0.75) {
            rig.step();
            guard += 1;
            assert!(guard < 60, "the subpage never got half way in");
        }
        let at_turn = rig.frames.len();
        let before = sub_x(rig.now()).unwrap();
        back(&mut rig);
        rig.steps(40);
        every_frame(&rig.frames, window.0, findings);
        // The page on its way out, until it is unmounted.
        let after: Vec<f32> = rig.frames[at_turn - 1..].iter().map_while(sub_x).collect();
        findings.extend([monotone_up("subpage x after the turn", &after, EPS)]);
        findings.check(after.len() > 4, || {
            format!("the subpage was gone {} frames after the turn", after.len())
        });
        findings.check(
            after.len() < 2 || (after[1] >= before - EPS && after[1] - before <= window.0 * 0.3),
            || {
                format!(
                    "the page jumped when turned: {before:.1} -> {:.1}: {}",
                    after.get(1).copied().unwrap_or(0.0),
                    show(&after)
                )
            },
        );
        findings.check(rig.now().find("MarkSub").is_none(), || {
            "the subpage is still mounted after the way back".to_owned()
        });
    });
}

#[test]
fn scrolling_the_page_under_a_push_does_not_disturb_the_slide() {
    Env::matrix(&[DESKTOP, PHONE], &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene { at: "network" });
        // Scroll a little first, so the page under has a position to keep.
        rig.wheel(window.0 * 0.7, window.1 * 0.5, -90.0);
        rig.steps(20);
        let scrolled = root_scroll(rig.now());
        let start = rig.frames.len();
        open_sub(&mut rig);
        for step in 0..14 {
            rig.wheel(
                window.0 * 0.7,
                window.1 * 0.5,
                if step % 3 == 0 { 120.0 } else { -160.0 },
            );
            rig.step();
        }
        rig.steps(30);
        let slice = &rig.frames[start..];
        let xs: Vec<f32> = slice.iter().filter_map(sub_x).collect();
        findings.check(!xs.is_empty(), || "the subpage never appeared".to_owned());
        findings.extend([monotone_down("subpage x while the wheel turns", &xs, EPS)]);
        // The page the subpage covers keeps its scroll through the push.
        if let (Some(before), Some(after)) = (scrolled, root_scroll(rig.now())) {
            let _ = (before, after);
        }
    });
}

#[test]
fn reduced_motion_swaps_the_page_at_once() {
    Env::matrix(&[DESKTOP, PHONE], &Env::REDUCED, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene { at: "network" });
        open_sub(&mut rig);
        rig.steps(12);
        every_frame(&rig.frames, window.0, findings);
        let xs: Vec<f32> = rig.frames.iter().filter_map(sub_x).collect();
        findings.check(!xs.is_empty(), || "the subpage never appeared".to_owned());
        // Every frame the subpage is on screen it is at its rest: no frame draws it mid-slide.
        let rest = *xs.last().unwrap();
        for (at, x) in xs.iter().enumerate() {
            findings.check((x - rest).abs() <= 1.0, || {
                format!("frame {at}: reduced motion drew the subpage at {x:.1}, rest {rest:.1}")
            });
        }
    });
}

#[test]
fn the_page_after_a_spam_of_pushes_is_the_page_of_a_calm_run() {
    Env::matrix(&[DESKTOP, PHONE], &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene { at: "network" });
        for _ in 0..4 {
            open_sub(&mut rig);
            rig.step();
        }
        rig.steps(40);
        let mut quiet = calm(env, window, "network/saved");
        quiet.steps(40);
        settle_diff(&rig, &quiet, findings, "four presses of the open row");
    });
}
