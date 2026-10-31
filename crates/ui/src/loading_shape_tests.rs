//! A placeholder has the shape of what replaces it (docs/settings.md, "Loading").
//!
//! A page draws its final layout on its first frame from placeholder rows, and
//! only values change when the data lands. These tests render a list twice, as
//! its placeholder and as the loaded rows, and compare the box of every group on
//! the page: same count, same position, same size. They also hold the rule
//! for what sits under a page (the Related group): it waits while a stand-in is
//! on the screen, so the page that replaces the stand-in does not push it down.

use std::time::Duration;

use freya::prelude::*;

use crate::group::{Group, caption, skeleton_caption, skeleton_row, skeleton_row_subtitled};
use crate::layout::page_column;
use crate::rows::row;
use crate::snapshot::Snapshot;
use crate::style::*;

const FRAME: Duration = Duration::from_millis(16);

/// A group's box as the page lays it out: where it is and how big.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Box4 {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// The box of every group on the page, top to bottom.
fn group_boxes(shot: &mut Snapshot) -> Vec<Box4> {
    let tree = crate::inspector::snapshot::Snapshot::capture_headless(shot.runner(), 1.0);
    let mut boxes: Vec<Box4> = tree
        .nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some("Group"))
        .filter_map(|node| node.area)
        .map(|area| Box4 {
            x: area.min_x(),
            y: area.min_y(),
            w: area.width(),
            h: area.height(),
        })
        .collect();
    boxes.sort_by(|a, b| a.y.total_cmp(&b.y));
    boxes
}

fn labels(shot: &mut Snapshot) -> Vec<String> {
    shot.runner()
        .find_many(|_, element| match Label::try_downcast(element) {
            Some(label) => Some(label.text.to_string()),
            None => Paragraph::try_downcast(element).map(|paragraph| {
                paragraph
                    .spans
                    .iter()
                    .map(|span| span.text.to_string())
                    .collect()
            }),
        })
}

fn has(shot: &mut Snapshot, text: &str) -> bool {
    labels(shot).iter().any(|label| label == text)
}

/// What a page puts in its list, as placeholder or as loaded rows.
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    /// Rows of one line.
    Plain,
    /// The first row has a second line, as a chosen zone's offset or a font's
    /// sample does, and a caption follows the list.
    Subtitled,
}

/// A page of one list under a sentinel group, so a list that is too tall or too
/// short moves the sentinel.
#[derive(Clone, PartialEq)]
struct Page {
    shape: Shape,
    loaded: bool,
}

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let tokens = crate::theme::provide_tokens();
        let list = match (self.shape, self.loaded) {
            (Shape::Plain, false) => {
                Group::new(None::<String>).rows((0..4).map(|_| skeleton_row()))
            }
            (Shape::Plain, true) => Group::new(None::<String>)
                .rows((0..4).map(|index| row(format!("Row {index}")).into_element())),
            (Shape::Subtitled, false) => Group::new(None::<String>)
                .row(skeleton_row_subtitled())
                .rows((1..4).map(|_| skeleton_row())),
            (Shape::Subtitled, true) => Group::new(None::<String>)
                .row(row("Chosen").subtitle("UTC+01:00"))
                .rows((1..4).map(|index| row(format!("Row {index}")).into_element())),
        };
        let tail: Element = match (self.shape, self.loaded) {
            (Shape::Plain, _) => rect().into(),
            (Shape::Subtitled, false) => skeleton_caption(),
            (Shape::Subtitled, true) => caption("12 more. Keep typing to narrow the list."),
        };
        rect()
            .expanded()
            .tw(&tokens, &[p(S4)])
            .child(crate::shell::OverlayHost::new(
                page_column()
                    .child(list)
                    .child(tail)
                    .child(Group::titled("After").row(row("Sentinel"))),
            ))
    }
}

fn boxes_of(shape: Shape, loaded: bool, size: (f32, f32)) -> Vec<Box4> {
    let mut shot = Snapshot::new(move || Page { shape, loaded }.into(), size);
    shot.settle(Duration::from_millis(600));
    group_boxes(&mut shot)
}

/// The placeholder and the loaded rows lay out the same groups in the same
/// places, at a desktop size and at a phone size.
fn assert_same_shape(shape: Shape) {
    for size in [(900.0, 700.0), (390.0, 800.0)] {
        let placeholder = boxes_of(shape, false, size);
        let loaded = boxes_of(shape, true, size);
        assert_eq!(
            placeholder.len(),
            loaded.len(),
            "at {size:?} the placeholder draws {} groups and the loaded page {}",
            placeholder.len(),
            loaded.len()
        );
        assert!(
            loaded.len() >= 2,
            "the list and the sentinel group are on the page"
        );
        for (index, (before, after)) in placeholder.iter().zip(&loaded).enumerate() {
            for (name, a, b) in [
                ("x", before.x, after.x),
                ("y", before.y, after.y),
                ("width", before.w, after.w),
                ("height", before.h, after.h),
            ] {
                assert!(
                    (a - b).abs() < 0.5,
                    "at {size:?} group {index} changes its {name} when the data lands: \
                     {a} as a placeholder, {b} loaded (placeholder {placeholder:?}, loaded {loaded:?})"
                );
            }
        }
    }
}

#[test]
fn a_list_of_one_line_rows_keeps_every_box_when_its_rows_land() {
    assert_same_shape(Shape::Plain);
}

#[test]
fn a_list_with_a_subtitled_first_row_and_a_caption_keeps_every_box_when_its_rows_land() {
    assert_same_shape(Shape::Subtitled);
}

/// A host of a page with the Related group under it, whose page shows `stand_in`
/// until the "Load" button is pressed and then the loaded rows.
#[derive(PartialEq)]
struct Hosted {
    stand_in: Stand,
}

#[derive(Clone, Copy, PartialEq)]
enum Stand {
    /// A skeleton row.
    Skeleton,
    /// Nothing drawn, announced with `use_provisional`, as a page that is
    /// hidden while it loads does.
    Hidden,
}

/// The page under the host.
#[derive(PartialEq)]
struct Loading {
    stand_in: Stand,
    loaded: State<bool>,
}

impl Component for Loading {
    fn render(&self) -> impl IntoElement {
        let loaded = *self.loaded.read();
        crate::stage::use_provisional(self.stand_in == Stand::Hidden && !loaded);
        let mut set = self.loaded;
        let content: Element = if loaded {
            Group::titled("Loaded")
                .rows((0..3).map(|i| row(format!("Loaded row {i}")).into_element()))
                .into()
        } else if self.stand_in == Stand::Skeleton {
            skeleton_row()
        } else {
            rect().into()
        };
        page_column()
            .child(row("Load").on_press(move |_| set.set(true)))
            .child(content)
    }
}

impl Component for Hosted {
    fn render(&self) -> impl IntoElement {
        crate::stage::provide_page();
        let tokens = crate::theme::provide_tokens();
        let loaded = use_state(|| false);
        rect()
            .expanded()
            .tw(&tokens, &[p(S4)])
            .child(crate::shell::OverlayHost::new(
                rect()
                    .child(Loading {
                        stand_in: self.stand_in,
                        loaded,
                    })
                    .child(crate::stage::after_page(
                        Group::titled("Related").row(row("Related row")),
                    )),
            ))
    }
}

fn under_a_stand_in(stand_in: Stand) {
    let mut shot = Snapshot::new(move || Hosted { stand_in }.into(), (700.0, 900.0));
    for _ in 0..40 {
        shot.settle(FRAME);
        assert!(
            !has(&mut shot, "Related row"),
            "the Related group is on screen while a stand-in holds the page's place"
        );
    }
    assert!(shot.click_text("Load"), "the Load row is on screen");
    let mut frames = 0;
    while !has(&mut shot, "Related row") {
        frames += 1;
        assert!(
            frames < 120,
            "the Related group never mounted after the page loaded"
        );
        shot.settle(FRAME);
    }
    assert!(
        has(&mut shot, "Loaded row 2"),
        "the Related group appears only once the page that replaces the stand-in is there"
    );
}

#[test]
fn what_sits_under_a_page_waits_while_a_skeleton_holds_the_pages_place() {
    under_a_stand_in(Stand::Skeleton);
}

#[test]
fn what_sits_under_a_page_waits_while_the_page_is_hidden_and_loading() {
    under_a_stand_in(Stand::Hidden);
}
