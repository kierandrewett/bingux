//! The focus ring's blur in and out, frame by frame: gaining focus, losing it,
//! moving it from one control to another, turning a change around half way,
//! and the per-corner radii a face gives its ring.
//!
//! Each test steps the headless app one 16 ms frame at a time and reads the
//! rings the way the ring check does: the ring node's opacity is its progress,
//! its box is the face grown by the outset, and its shadow is the blur.

use std::time::Duration;

use freya::prelude::*;

use crate::component::ComponentName;
use crate::motion::{
    FOCUS_BLUR, FOCUS_GAP, FOCUS_IN_MS, FOCUS_OUT_MS, FOCUS_TRAVEL, FOCUS_WIDTH, Focusable, RingLook,
};
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::{Scheme, Tokens, provide_tokens};

const FRAME: Duration = Duration::from_millis(16);
const SIZE: (f32, f32) = (400.0, 300.0);
/// The three buttons that move focus: to the first face, the second, nowhere.
const ROW: f32 = 30.0;

#[derive(PartialEq)]
struct Scene;

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let mut which = use_state(|| 0_usize);
        let button = move |to: usize| {
            rect()
                .width(Size::fill())
                .height(Size::px(ROW))
                .on_press(move |_| which.set(to))
        };
        let face = |name: &'static str, on: bool, radii: Corners| {
            rect()
                .component(name)
                .tw(
                    &tokens,
                    &[
                        w(Dim::DropdownMenu),
                        h(Dim::Control),
                        bg(Paint::Control),
                        rounded_on(Radius::Control, radii, 0.0),
                    ],
                )
                .focusable(if on { Focus::Keyboard } else { Focus::Not })
        };
        rect()
            .expanded()
            .tw(&tokens, &[flex_col()])
            .child(button(1))
            .child(button(2))
            .child(button(0))
            .child(
                rect()
                    .tw(&tokens, &[flex_col(), gap(S5), p(S5)])
                    .child(face("FaceOne", which() == 1, Corners::All))
                    .child(face("FaceTwo", which() == 2, Corners::All)),
            )
    }
}

/// What one ring looks like in one frame.
#[derive(Clone, Copy, Debug)]
struct Seen {
    progress: f32,
    /// How far outside the face the ring's box reaches.
    outset: f32,
    blurred: bool,
}

fn seen(shot: &mut Snapshot, name: &str) -> Option<Seen> {
    shot.focus_rings().into_iter().find(|probe| probe.label.contains(name)).map(|probe| Seen {
        progress: probe.progress,
        outset: (probe.face[0] - probe.ring[0]).abs(),
        blurred: probe.blurred,
    })
}

fn mount() -> Snapshot {
    let mut shot = Snapshot::new(|| Scene.into(), SIZE);
    shot.settle(Duration::from_millis(200));
    shot
}

/// Clicks the button for focus target `to` (0 clears focus).
fn move_focus(shot: &mut Snapshot, to: usize) {
    let index = [2, 0, 1][to] as f32;
    shot.click(50.0, index * ROW + ROW / 2.0);
}

fn frames(shot: &mut Snapshot, name: &str, count: usize) -> Vec<Seen> {
    (0..count)
        .map(|_| {
            shot.settle(FRAME);
            seen(shot, name).expect("the ring exists")
        })
        .collect()
}

const IN_FRAMES: usize = (FOCUS_IN_MS / 16) as usize + 3;
const OUT_FRAMES: usize = (FOCUS_OUT_MS / 16) as usize + 3;
const SETTLED: f32 = FOCUS_GAP + FOCUS_WIDTH;

#[test]
fn a_control_that_was_never_focused_has_no_ring() {
    let mut shot = mount();
    assert!(seen(&mut shot, "FaceOne").is_none());
}

#[test]
fn gaining_focus_blurs_in_from_a_wider_ring_to_a_crisp_one() {
    let mut shot = mount();
    move_focus(&mut shot, 1);
    let run = frames(&mut shot, "FaceOne", IN_FRAMES);
    eprintln!("{run:#?}");
    assert!(run.windows(2).all(|pair| pair[1].progress >= pair[0].progress), "fades in only");
    assert!(run.windows(2).all(|pair| pair[1].outset <= pair[0].outset + 0.01), "comes in only");
    let first = run[0];
    assert!(first.progress < 0.9 && first.outset > SETTLED + 0.5, "starts soft and wide: {first:?}");
    assert!(run.iter().any(|seen| seen.blurred), "blurred on the way in");
    let last = *run.last().unwrap();
    assert!((last.progress - 1.0).abs() < 0.01 && (last.outset - SETTLED).abs() < 0.5, "{last:?}");
    assert!(!last.blurred, "settles crisp");
}

#[test]
fn losing_focus_blurs_out_and_stays_until_it_is_gone() {
    let mut shot = mount();
    move_focus(&mut shot, 1);
    shot.settle(Duration::from_millis(400));
    move_focus(&mut shot, 0);
    let run = frames(&mut shot, "FaceOne", OUT_FRAMES);
    eprintln!("{run:#?}");
    assert!(run.windows(2).all(|pair| pair[1].progress <= pair[0].progress), "fades out only");
    assert!(run.windows(2).all(|pair| pair[1].outset >= pair[0].outset - 0.01), "goes out only");
    // It does not vanish in one frame.
    assert!(run[0].progress > 0.3, "{:?}", run[0]);
    assert!(run.iter().any(|seen| seen.blurred), "softens while it goes");
    let last = *run.last().unwrap();
    assert!(last.progress < 0.01 && (last.outset - (SETTLED + FOCUS_TRAVEL)).abs() < 0.5, "{last:?}");
}

#[test]
fn moving_focus_cross_fades_the_old_ring_out_while_the_new_one_comes_in() {
    let mut shot = mount();
    move_focus(&mut shot, 1);
    shot.settle(Duration::from_millis(400));
    move_focus(&mut shot, 2);
    let old = frames(&mut shot, "FaceOne", 4);
    let new = {
        let mut shot2 = mount();
        move_focus(&mut shot2, 1);
        shot2.settle(Duration::from_millis(400));
        move_focus(&mut shot2, 2);
        frames(&mut shot2, "FaceTwo", 4)
    };
    eprintln!("old {old:#?}\nnew {new:#?}");
    // In the same frames, one is on its way out and the other on its way in.
    assert!(old[3].progress < old[0].progress && old[3].progress < 1.0);
    assert!(new[3].progress > new[0].progress && new[0].progress < 1.0);
    // At the middle both are partly there: the focus is never ringless.
    assert!(old[2].progress > 0.0 && new[2].progress > 0.0);
}

#[test]
fn turning_a_change_around_continues_from_where_the_ring_is() {
    let mut shot = mount();
    move_focus(&mut shot, 1);
    let rise = frames(&mut shot, "FaceOne", 4);
    let at_turn = rise[3].progress;
    assert!(at_turn > 0.0 && at_turn < 1.0, "turned mid-animation: {at_turn}");
    move_focus(&mut shot, 0);
    let after = frames(&mut shot, "FaceOne", 3);
    // No snap: the first frame after the turn is within one frame's step of
    // where it was, and it then only falls.
    assert!((after[0].progress - at_turn).abs() < 0.35, "{at_turn} then {:?}", after[0]);
    assert!(after.windows(2).all(|pair| pair[1].progress <= pair[0].progress));
    // And back in again from there.
    move_focus(&mut shot, 1);
    let again = frames(&mut shot, "FaceOne", IN_FRAMES);
    assert!(again.windows(2).all(|pair| pair[1].progress >= pair[0].progress));
    assert!((again.last().unwrap().progress - 1.0).abs() < 0.01);
}

#[test]
fn reduced_motion_makes_the_ring_instant() {
    let reduced = Tokens::new("blue", Scheme::PreferDark, true);
    assert_eq!(reduced.motion(FOCUS_IN_MS), 0);
    assert_eq!(reduced.motion(FOCUS_OUT_MS), 0);
    let normal = Tokens::new("blue", Scheme::PreferDark, false);
    assert_eq!(normal.motion(FOCUS_IN_MS), FOCUS_IN_MS);
}

#[test]
fn the_look_is_a_pure_function_of_one_progress() {
    let gone = RingLook::at(0.0);
    assert_eq!((gone.opacity, gone.travel, gone.blur), (0.0, FOCUS_TRAVEL, FOCUS_BLUR));
    let settled = RingLook::at(1.0);
    assert_eq!((settled.opacity, settled.travel, settled.blur, settled.spread), (1.0, 0.0, 0.0, 0.0));
    let half = RingLook::at(0.5);
    assert_eq!((half.opacity, half.travel, half.blur), (0.5, FOCUS_TRAVEL / 2.0, FOCUS_BLUR / 2.0));
    // Out of range clamps.
    assert_eq!(RingLook::at(2.0), settled);
    assert_eq!(RingLook::at(-1.0), gone);
}

#[test]
fn a_ring_takes_each_corner_of_its_face() {
    #[derive(PartialEq)]
    struct Mixed;
    impl Component for Mixed {
        fn render(&self) -> impl IntoElement {
            let tokens = provide_tokens();
            let face = |name: &'static str, corners: Corners| {
                rect()
                    .component(name)
                    .tw(
                        &tokens,
                        &[
                            w(Dim::DropdownMenu),
                            h(Dim::Control),
                            bg(Paint::Control),
                            rounded_on(Radius::Control, corners, 0.0),
                        ],
                    )
                    .focusable(Focus::Keyboard)
            };
            rect()
                .tw(&tokens, &[flex_col(), p(S5), gap(S5)])
                .child(face("Square", Corners::None))
                .child(face("Round", Corners::All))
                .child(face("Tab", Corners::Top))
                .child(face("Left", Corners::Left))
                .child(face("Corner", Corners::BottomRight))
        }
    }
    let mut shot = Snapshot::new(|| Mixed.into(), (400.0, 400.0));
    shot.settle(Duration::from_millis(500));
    let probes = shot.focus_rings();
    assert_eq!(probes.len(), 5);
    let want = |name: &str| probes.iter().find(|probe| probe.label.contains(name)).expect(name);
    let out = FOCUS_GAP + FOCUS_WIDTH;
    let round = Radius::Control.px();
    assert_eq!(want("Square").ring_radii, [0.0; 4]);
    assert_eq!(want("Round").ring_radii, [round + out; 4]);
    assert_eq!(want("Tab").ring_radii, [round + out, round + out, 0.0, 0.0]);
    assert_eq!(want("Left").ring_radii, [round + out, 0.0, 0.0, round + out]);
    assert_eq!(want("Corner").ring_radii, [0.0, 0.0, round + out, 0.0]);
    assert!(crate::focus::ring_check::check(&probes).is_empty());
}

#[test]
fn a_square_face_given_a_round_ring_is_caught_by_the_check() {
    #[derive(PartialEq)]
    struct Liar;
    impl Component for Liar {
        fn render(&self) -> impl IntoElement {
            let tokens = provide_tokens();
            // The painter is the inner box, which is rounded; the face given
            // to `focusable` is the square wrapper around it.
            rect()
                .tw(&tokens, &[p(S5)])
                .child(
                    rect()
                        .component("Wrapper")
                        .child(
                            rect()
                                .tw(
                                    &tokens,
                                    &[
                                        w(Dim::DropdownMenu),
                                        h(Dim::Control),
                                        bg(Paint::Control),
                                        rounded(Radius::Control),
                                    ],
                                ),
                        )
                        .focusable(Focus::Keyboard),
                )
        }
    }
    let mut shot = Snapshot::new(|| Liar.into(), (300.0, 200.0));
    shot.settle(Duration::from_millis(500));
    let found = shot.ring_mismatches();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].what, "a square ring corner around a rounded face corner");
}

mod evidence {
    //! Frame strips and the Files list, written as PNGs when `BINGUX_FRAME_DIR`
    //! is set: `cargo test --lib --features testing ring_evidence -- --ignored`.

    use std::path::PathBuf;
    use std::sync::Arc;

    use bingux_files::{Entry, Kind, Listing, Selection};

    use super::*;
    use crate::files::FileView;
    use crate::shell::OverlayHost;

    fn dir() -> Option<PathBuf> {
        std::env::var_os("BINGUX_FRAME_DIR").map(PathBuf::from)
    }

    fn strip(shot: &mut Snapshot, dir: &std::path::Path, name: &str, frames: usize) {
        for index in 0..frames {
            shot.settle(FRAME);
            shot.save(dir.join(format!("{name}-{index:02}.png")));
        }
    }

    #[test]
    #[ignore = "writes PNGs when BINGUX_FRAME_DIR is set"]
    fn ring_evidence_strips() {
        let Some(dir) = dir() else { return };
        let mut shot = mount();
        move_focus(&mut shot, 1);
        strip(&mut shot, &dir, "gain", 12);
        shot.settle(Duration::from_millis(300));
        move_focus(&mut shot, 2);
        strip(&mut shot, &dir, "swap", 12);
        shot.settle(Duration::from_millis(300));
        move_focus(&mut shot, 0);
        strip(&mut shot, &dir, "loss", 10);
    }

    #[derive(PartialEq)]
    struct Window {
        corners: Corners,
    }

    impl Component for Window {
        fn render(&self) -> impl IntoElement {
            let tokens = provide_tokens();
            let corners = self.corners;
            let selection = use_state(Selection::default);
            let level = use_state(|| 16_u16);
            let (listing, order) = use_hook(|| {
                let mut listing = Listing::new("/evidence");
                for index in 0..60 {
                    let name = format!("Screencast from 2023-10-{index:02} 15-34-25.webm");
                    listing.entries.push(Entry::named(
                        PathBuf::from("/evidence").join(&name),
                        name,
                        Kind::File,
                        false,
                        false,
                    ));
                }
                listing.complete = true;
                (Arc::new(listing), Arc::new((0..60).collect::<Vec<usize>>()))
            });
            // A window: its card clips, so its corners are rounded, and the
            // file view rests in the bottom right corner beside a sidebar.
            rect().expanded().tw(&tokens, &[p(S5)]).child(
                rect()
                    .expanded()
                    .tw(&tokens, &[flex_row(), clip(), rounded(Radius::Window), bg(Paint::Card)])
                    .child(rect().tw(&tokens, &[w(Dim::PlacesColumn), h_full(), bg(Paint::Control)]))
                    .child(
                        rect().tw(&tokens, &[w_flex(), h_full()]).child(OverlayHost::new(
                            FileView::new(listing, order, selection)
                                .icon_px(level)
                                .thumbnails(false)
                                .autofocus(true)
                                .corners(corners),
                        )),
                    ),
            )
        }
    }

    #[test]
    #[ignore = "writes PNGs when BINGUX_FRAME_DIR is set"]
    fn ring_evidence_files_list() {
        let Some(dir) = dir() else { return };
        for (name, corners) in [("square", Corners::None), ("window-corner", Corners::BottomRight)] {
            let mut shot = Snapshot::new(move || Window { corners }.into(), (1100.0, 820.0));
            shot.settle(Duration::from_millis(500));
            shot.key(Key::Named(NamedKey::Tab));
            shot.settle(Duration::from_millis(600));
            shot.save(dir.join(format!("files-list-{name}.png")));
            let rings = shot.focus_rings();
            eprintln!("{name}: {rings:#?}");
            assert!(shot.ring_mismatches().is_empty());
        }
    }
}
