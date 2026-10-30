//! What a page costs the frame that mounts it. These pin the shape of the work
//! the frame gate measures: a row mounts without a focus ring, a long page and
//! a long group come in a piece a frame and never out of order, and every
//! piece of text and control the page has is there once it has settled.

use std::time::Duration;

use freya::prelude::*;

use crate::group::Group;
use crate::layout::page_column;
use crate::rows::row;
use crate::snapshot::Snapshot;
use crate::stage::{FIRST_ROWS, MORE_ROWS, staged};
use crate::style::*;

const FRAME: Duration = Duration::from_millis(16);

/// The text of every label in the window.
fn labels(shot: &mut Snapshot) -> Vec<String> {
    shot.runner().find_many(|_, element| match Label::try_downcast(element) {
        Some(label) => Some(label.text.to_string()),
        None => Paragraph::try_downcast(element).map(|paragraph| {
            paragraph.spans.iter().map(|span| span.text.to_string()).collect()
        }),
    })
}

fn has(shot: &mut Snapshot, text: &str) -> bool {
    labels(shot).iter().any(|label| label == text)
}

/// The opacity of every focus ring in the window.
fn ring_opacities(shot: &mut Snapshot) -> Vec<f32> {
    shot.runner().find_many(|_, element| {
        (element
            .accessibility()
            .builder
            .class_name()
            .is_some_and(|name| name.starts_with("FocusRing")))
            .then(|| element.effect().and_then(|effect| effect.opacity).unwrap_or(1.0))
    })
}

fn tab() -> Key {
    Key::Named(NamedKey::Tab)
}

#[derive(PartialEq)]
struct Rows;

impl Component for Rows {
    fn render(&self) -> impl IntoElement {
        let tokens = crate::theme::provide_tokens();
        rect()
            .expanded()
            .tw(&tokens, &[p(S4)])
            .child(crate::shell::OverlayHost::new(
                Group::new(None::<String>).rows((0..3).map(|index| {
                    row(format!("Row {index}"))
                        .navigates()
                        .on_press(|_| {})
                        .into_element()
                })),
            ))
    }
}

#[test]
fn a_row_has_no_focus_ring_until_keyboard_focus_reaches_it() {
    let mut shot = Snapshot::new(|| Rows.into(), (600.0, 400.0));
    shot.settle(Duration::from_millis(300));
    assert!(
        ring_opacities(&mut shot).is_empty(),
        "rows that were never focused carry no ring"
    );

    shot.key(tab()).settle(Duration::from_millis(400));
    let rings = ring_opacities(&mut shot);
    assert_eq!(rings.len(), 1, "the focused row has its ring: {rings:?}");
    assert!(rings[0] > 0.95, "the ring has arrived: {rings:?}");

    shot.key(tab()).settle(Duration::from_millis(400));
    let rings = ring_opacities(&mut shot);
    assert_eq!(
        rings.iter().filter(|opacity| **opacity > 0.5).count(),
        1,
        "focus is on one row: {rings:?}"
    );
    assert_eq!(
        rings.iter().filter(|opacity| **opacity < 0.05).count(),
        rings.len() - 1,
        "the row focus left has faded its ring out: {rings:?}"
    );
}

/// The column of a page of `groups` groups of `rows` rows each, under a head.
fn column(groups: usize, rows: usize) -> Rect {
    let column = page_column().child(Group::titled("Head").row(row("Head row")));
    (0..groups).fold(column, |column, group| {
        column.child(
            Group::titled(&format!("Group {group}")).rows(
                (0..rows).map(|index| row(format!("Row {group}.{index}")).into_element()),
            ),
        )
    })
}

/// A page of `groups` groups of `rows` rows each, under a title.
fn page(groups: usize, rows: usize) -> Element {
    let tokens = crate::theme::provide_tokens();
    rect()
        .expanded()
        .tw(&tokens, &[p(S4)])
        .child(crate::shell::OverlayHost::new(staged(column(groups, rows))))
        .into()
}

#[test]
fn a_page_mounts_its_first_groups_and_the_rest_a_group_a_frame() {
    let mut shot = Snapshot::new(|| page(6, 2), (700.0, 900.0));
    assert!(has(&mut shot, "Head"), "the head mounts with the page");
    assert!(has(&mut shot, "Group 0"), "so does the first group");
    assert!(
        !has(&mut shot, "Group 5"),
        "the last group waits for its turn"
    );
    shot.settle(Duration::from_millis(1000));
    for group in 0..6 {
        assert!(
            has(&mut shot, &format!("Group {group}")),
            "Group {group} is there once the page has settled"
        );
        assert!(
            has(&mut shot, &format!("Row {group}.1")),
            "labels: {:?}",
            labels(&mut shot)
        );
    }
}

#[test]
fn a_long_group_adds_rows_and_what_is_below_it_waits_for_the_last_one() {
    // Longer than the rows that mount with a group, so it is staged.
    let rows = FIRST_ROWS + 2 * MORE_ROWS;
    let mut shot = Snapshot::new(move || page(3, rows), (700.0, 2000.0));
    let mut frames = 0;
    let mut first_rows = None;
    loop {
        let mounted = labels(&mut shot)
            .iter()
            .filter(|label| label.starts_with("Row 0."))
            .count();
        first_rows.get_or_insert(mounted);
        let below = has(&mut shot, "Group 1");
        assert!(
            !below || mounted == rows,
            "frame {frames}: Group 1 is mounted while Group 0 has {mounted} of its {rows} rows"
        );
        if mounted == rows && has(&mut shot, &format!("Row 2.{}", rows - 1)) {
            break;
        }
        frames += 1;
        assert!(frames < 120, "the page never became whole");
        shot.settle(FRAME);
    }
    assert!(
        first_rows.is_some_and(|mounted| mounted < rows),
        "the long group does not mount all of its rows with the page"
    );
}

/// A host of a page with something under it, as a Settings page with its Related
/// group.
#[derive(PartialEq)]
struct Hosted {
    groups: usize,
    rows: usize,
}

impl Component for Hosted {
    fn render(&self) -> impl IntoElement {
        crate::stage::provide_page();
        let tokens = crate::theme::provide_tokens();
        rect()
            .expanded()
            .tw(&tokens, &[p(S4)])
            .child(crate::shell::OverlayHost::new(
                rect()
                    .child(staged(column(self.groups, self.rows)))
                    .child(crate::stage::after_page(
                        Group::titled("Related").row(row("Related row")),
                    )),
            ))
    }
}

#[test]
fn what_sits_under_a_staged_page_waits_for_the_page_to_be_whole() {
    // The last group is long, so the page is still filling in after its last
    // child has mounted.
    let rows = FIRST_ROWS + MORE_ROWS;
    let mut shot = Snapshot::new(move || Hosted { groups: 4, rows }.into(), (700.0, 2000.0));
    let last = format!("Row 3.{}", rows - 1);
    let mut frames = 0;
    let mut early = false;
    loop {
        let whole = has(&mut shot, &last);
        let tail = has(&mut shot, "Related row");
        assert!(
            !tail || whole,
            "frame {frames}: the Related group is there before the page is whole"
        );
        early |= !tail;
        if tail {
            break;
        }
        frames += 1;
        assert!(frames < 200, "the Related group never mounted");
        shot.settle(FRAME);
    }
    assert!(early, "the Related group mounted with the first frame of the page");
}

#[test]
fn what_sits_under_a_page_that_is_not_staged_mounts_at_once() {
    let mut shot = Snapshot::new(|| Hosted { groups: 1, rows: 2 }.into(), (700.0, 900.0));
    assert!(
        has(&mut shot, "Related row"),
        "a short page has nothing to wait for: {:?}",
        labels(&mut shot)
    );
}
