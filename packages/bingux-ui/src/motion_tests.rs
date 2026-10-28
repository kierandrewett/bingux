//! Reveal, frame by frame: the height of a revealed block through an open, a
//! close and a close that is turned around half way. Each test steps the
//! headless app one 16 ms frame at a time and reads where the box below the
//! reveal sits.

use std::time::Duration;

use freya::prelude::*;

use super::*;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;

const FRAME: Duration = Duration::from_millis(16);
const SIZE: (f32, f32) = (400.0, 500.0);
/// The header that toggles the reveal.
const HEADER: f32 = 40.0;
/// The revealed content.
const BODY: f32 = 120.0;
/// The marker box below the reveal, whose top edge tracks the reveal's height.
const MARKER: f32 = 24.0;

#[derive(PartialEq, Clone, Copy)]
struct Scene {
    start_open: bool,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let start = self.start_open;
        let mut open = use_state(move || start);
        rect()
            .expanded()
            .tw(&tokens, &[flex_col()])
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(HEADER))
                    .on_press(move |_| open.set(!open())),
            )
            .child(reveal(
                open(),
                rect()
                    .width(Size::fill())
                    .height(Size::px(BODY))
                    .tw(&tokens, &[bg(Paint::Danger)]),
            ))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(MARKER))
                    .tw(&tokens, &[bg(Paint::Accent)]),
            )
    }
}

fn shot(start_open: bool) -> Snapshot {
    let mut shot = Snapshot::new(move || Scene { start_open }.into(), SIZE);
    shot.settle(Duration::from_millis(100));
    shot
}

/// The top edge of the marker box now.
fn marker_top(shot: &mut Snapshot) -> f32 {
    shot.runner()
        .find_many(|node, _| {
            let area = node.layout().area;
            ((area.height() - MARKER).abs() < 0.01 && area.width() > 100.0).then(|| area.min_y())
        })
        .first()
        .copied()
        .expect("the marker box")
}

fn click_header(shot: &mut Snapshot) {
    shot.click(50.0, HEADER / 2.0);
}

/// The marker's top edge after each of `frames` frames.
fn run(shot: &mut Snapshot, frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|_| {
            shot.settle(FRAME);
            marker_top(shot)
        })
        .collect()
}

#[test]
fn a_reveal_that_mounts_closed_or_open_does_not_animate() {
    let mut closed = shot(false);
    assert_eq!(marker_top(&mut closed), HEADER);
    let mut open = shot(true);
    assert_eq!(marker_top(&mut open), HEADER + BODY);
}

#[test]
fn opening_grows_the_height_smoothly_to_the_content() {
    let mut shot = shot(false);
    click_header(&mut shot);
    let tops = run(&mut shot, 14);
    assert!(
        tops.windows(2).all(|pair| pair[1] >= pair[0]),
        "never shrinks: {tops:?}"
    );
    assert!(tops.iter().any(|top| *top > HEADER + 1.0 && *top < HEADER + BODY - 1.0));
    assert_eq!(*tops.last().unwrap(), HEADER + BODY);
    // No frame jumps by more than a third of the content: the edge moves
    // continuously.
    assert!(
        tops.windows(2).all(|pair| pair[1] - pair[0] < BODY / 3.0),
        "no jump: {tops:?}"
    );
}

#[test]
fn closing_shrinks_the_height_smoothly_with_no_empty_box() {
    let mut shot = shot(true);
    click_header(&mut shot);
    let tops = run(&mut shot, 14);
    assert!(
        tops.windows(2).all(|pair| pair[1] <= pair[0]),
        "never grows: {tops:?}"
    );
    assert!(tops.iter().any(|top| *top > HEADER + 1.0 && *top < HEADER + BODY - 1.0));
    assert_eq!(*tops.last().unwrap(), HEADER);
    assert!(
        tops.windows(2).all(|pair| pair[0] - pair[1] < BODY / 3.0),
        "no jump: {tops:?}"
    );
}

#[test]
fn heights_at_the_start_middle_and_end_differ_in_order() {
    let mut shot = shot(true);
    let start = marker_top(&mut shot);
    click_header(&mut shot);
    let tops = run(&mut shot, 14);
    let middle = tops[tops.len() / 4];
    let end = *tops.last().unwrap();
    assert!(start > middle && middle > end, "{start} > {middle} > {end}");
}

#[test]
fn turning_a_close_around_continues_from_the_current_height() {
    let mut shot = shot(true);
    click_header(&mut shot);
    let closing = run(&mut shot, 4);
    let turned_at = *closing.last().unwrap();
    assert!(turned_at < HEADER + BODY && turned_at > HEADER, "{turned_at}");
    click_header(&mut shot);
    let opening = run(&mut shot, 14);
    // The first frame after the turn is next to where it was, not at either end.
    assert!(
        (opening[0] - turned_at).abs() < BODY / 3.0,
        "turned at {turned_at}, next frame {}",
        opening[0]
    );
    assert!(
        opening.windows(2).all(|pair| pair[1] >= pair[0]),
        "grows again: {opening:?}"
    );
    assert_eq!(*opening.last().unwrap(), HEADER + BODY);
}

#[test]
fn the_height_curve_is_linear_in_progress_and_clamped() {
    assert_eq!(reveal_height(100.0, 0.0), 0.0);
    assert_eq!(reveal_height(100.0, 0.5), 50.0);
    assert_eq!(reveal_height(100.0, 1.0), 100.0);
    assert_eq!(reveal_height(100.0, 1.4), 100.0);
    assert_eq!(reveal_height(100.0, -0.4), 0.0);
    assert_eq!(reveal_height(-5.0, 0.5), 0.0);
}

#[test]
fn the_layers_fade_in_late_and_settle_in_place() {
    assert_eq!(reveal_layers(0.0), (0.0, -REVEAL_LIFT));
    assert_eq!(reveal_layers(1.0), (1.0, 0.0));
    let mut last = reveal_layers(0.0);
    for step in 1..=20 {
        let now = reveal_layers(step as f32 / 20.0);
        assert!(now.0 >= last.0 && now.1 >= last.1, "{last:?} -> {now:?}");
        last = now;
    }
    // The box is already moving before the first rows show.
    assert_eq!(reveal_layers(0.2).0, 0.0);
}

#[test]
fn a_settled_open_reveal_is_auto_sized_and_a_moving_one_is_measured() {
    let measured = Some(80.0);
    assert!(matches!(reveal_size(true, 1.0, measured, false), s if s == Size::auto()));
    assert!(matches!(reveal_size(true, 0.5, measured, false), s if s == Size::px(40.0)));
    assert!(matches!(reveal_size(false, 0.0, measured, false), s if s == Size::px(0.0)));
    // Not measured yet: an open one shows whole, a closed or lazy one waits.
    assert!(matches!(reveal_size(true, 0.5, None, false), s if s == Size::auto()));
    assert!(matches!(reveal_size(true, 0.5, None, true), s if s == Size::px(0.0)));
    assert!(matches!(reveal_size(false, 0.0, None, false), s if s == Size::px(0.0)));
}

/// An expander row followed by the marker box, so the marker's top edge is
/// the bottom of the expander.
#[derive(PartialEq, Clone, Copy)]
struct ExpanderScene {
    start_open: bool,
}

impl Component for ExpanderScene {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        rect()
            .expanded()
            .tw(&tokens, &[flex_col()])
            .child(
                crate::rows::expander_row(
                    "Advanced",
                    vec![
                        crate::rows::row("Option one").into(),
                        crate::rows::row("Option two").into(),
                    ],
                )
                .expanded(self.start_open),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(MARKER))
                    .tw(&tokens, &[bg(Paint::Accent)]),
            )
    }
}

fn expander(start_open: bool) -> Snapshot {
    let mut shot = Snapshot::new(move || ExpanderScene { start_open }.into(), SIZE);
    shot.settle(Duration::from_millis(100));
    shot
}

#[test]
fn an_expander_closes_by_animating_its_height_with_no_empty_box() {
    let mut shot = expander(true);
    let open = marker_top(&mut shot);
    click_header_at(&mut shot, 20.0);
    let tops = run(&mut shot, 14);
    let closed = *tops.last().unwrap();
    assert!(closed < open, "{closed} < {open}");
    assert!(tops.windows(2).all(|pair| pair[1] <= pair[0]), "{tops:?}");
    assert!(
        tops.iter().any(|top| *top < open - 2.0 && *top > closed + 2.0),
        "the edge passes between the two heights: {tops:?}"
    );
    // The first frame has already started to close.
    assert!(tops[2] < open, "{tops:?}");
}

#[test]
fn an_expander_opens_and_stays_open_at_its_full_height() {
    let mut shot = expander(false);
    let closed = marker_top(&mut shot);
    click_header_at(&mut shot, 20.0);
    let tops = run(&mut shot, 14);
    assert!(tops.windows(2).all(|pair| pair[1] >= pair[0]), "{tops:?}");
    let open = *tops.last().unwrap();
    assert!(open > closed + 60.0, "{closed} -> {open}");
}

fn click_header_at(shot: &mut Snapshot, y: f32) {
    shot.click(50.0, y);
}

// Frame strips for the record: `BINGUX_FRAME_DIR=/some/dir cargo test --lib
// --features testing frame_strips -- --ignored` writes one PNG per 16 ms frame
// of each motion, named `<scene>-<frame>.png`.
mod strips {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use super::*;
    use crate::controls::{TreeNode, tree_view};
    use crate::group::Group;
    use crate::rows::{expander_row, row};

    const DESKTOP: (f32, f32) = (520.0, 360.0);
    const PHONE: (f32, f32) = (390.0, 800.0);

    #[derive(PartialEq, Clone, Copy)]
    enum Kind {
        Expander,
        Nested,
        Tree,
    }

    #[derive(PartialEq, Clone, Copy)]
    struct Strip {
        kind: Kind,
        open: bool,
    }

    impl Component for Strip {
        fn render(&self) -> impl IntoElement {
            let tokens = provide_tokens();
            let start = self.open;
            let mut open = use_state(move || start);
            let body: Element = match self.kind {
                Kind::Expander => Group::titled("Closed, open it")
                    .rows(vec![
                        expander_row(
                            "Advanced",
                            vec![row("Option one").into(), row("Option two").into()],
                        )
                        .subtitle("Press to reveal two rows")
                        .expanded(start)
                        .into(),
                        row("Another setting").into(),
                    ])
                    .into(),
                Kind::Nested => Group::titled("Hardware")
                    .rows(vec![
                        row("Cloned MAC address")
                            .value("Address")
                            .on_press(move |_| open.set(!open()))
                            .nested(
                                open(),
                                vec![row("MAC address").mono_value("02:1A:2B:3C:4D:5E").into()],
                            )
                            .into(),
                        row("MTU").value("Automatic").into(),
                    ])
                    .into(),
                Kind::Tree => {
                    let expanded = use_state(move || {
                        if start { HashSet::from([1_u64]) } else { HashSet::new() }
                    });
                    let selected = use_state(|| None::<u64>);
                    let mut root = TreeNode::new(1, "Container");
                    root.children = (10..14)
                        .map(|id| TreeNode::new(id, format!("Child {}", id - 9)))
                        .collect();
                    tree_view(vec![root, TreeNode::new(2, "Sibling")], expanded, selected).into()
                }
            };
            rect()
                .expanded()
                .tw(&tokens, &[p(S4)])
                .child(crate::shell::OverlayHost::new(body))
        }
    }

    fn dir() -> Option<PathBuf> {
        std::env::var_os("BINGUX_FRAME_DIR").map(PathBuf::from)
    }

    /// Mount `scene`, click at `at`, and save `frames` frames (the first is
    /// the state before the click). `turn` clicks again after that many frames.
    fn record(
        name: &str,
        scene: Strip,
        size: (f32, f32),
        at: (f32, f32),
        frames: usize,
        turn: Option<usize>,
    ) {
        let Some(dir) = dir() else { return };
        let mut shot = Snapshot::new(move || scene.into(), size);
        shot.settle(Duration::from_millis(200));
        shot.save(dir.join(format!("{name}-00.png")));
        shot.click(at.0, at.1);
        for index in 1..=frames {
            if turn == Some(index) {
                shot.click(at.0, at.1);
            }
            shot.settle(FRAME);
            shot.save(dir.join(format!("{name}-{index:02}.png")));
        }
    }

    #[test]
    #[ignore = "writes frame strips when BINGUX_FRAME_DIR is set"]
    fn frame_strips() {
        let desktop = |size: (f32, f32)| (size.0 / 2.0, 16.0 + 28.0 + 24.0);
        for (label, size) in [("desktop", DESKTOP), ("phone", PHONE)] {
            let at = desktop(size);
            record(&format!("expander-close-{label}"), Strip { kind: Kind::Expander, open: true }, size, at, 14, None);
            record(&format!("expander-open-{label}"), Strip { kind: Kind::Expander, open: false }, size, at, 14, None);
            record(&format!("nested-open-{label}"), Strip { kind: Kind::Nested, open: false }, size, at, 14, None);
        }
        record("expander-interrupted", Strip { kind: Kind::Expander, open: true }, DESKTOP, desktop(DESKTOP), 16, Some(5));
        // The chevron of the first row.
        for (label, size) in [("desktop", DESKTOP), ("phone", PHONE)] {
            record(&format!("tree-expand-{label}"), Strip { kind: Kind::Tree, open: false }, size, (16.0 + 4.0 + 12.0, 16.0 + 14.0), 14, None);
        }
    }
}
