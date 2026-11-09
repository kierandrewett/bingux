//! The focus ring follows keyboard modality, as CSS `:focus-visible` does: a
//! press with the pointer focuses a field without a ring, a Tab draws one, and
//! a press after a Tab takes the ring away.

use std::time::Duration;

use freya::prelude::*;

use crate::controls::text_field::text_field;
use crate::focus::ring_check::RingProbe;
use crate::shell::overlays::OverlayHost;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

const SIZE: (f32, f32) = (400.0, 200.0);
/// Each field's row: the field sits at the row's top left.
const ROW: f32 = 48.0;

#[derive(PartialEq)]
struct Fields;

impl Component for Fields {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let first = use_state(String::new);
        let second = use_state(String::new);
        OverlayHost::new(
            rect()
                .expanded()
                .tw(&tokens, &[flex_col()])
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(ROW))
                        .child(text_field(first, |_| {})),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(ROW))
                        .child(text_field(second, |_| {})),
                ),
        )
    }
}

fn mount() -> Snapshot {
    let mut shot = Snapshot::new(|| Fields.into(), SIZE);
    shot.settle(Duration::from_millis(300));
    shot
}

/// The rings that are drawn now: a ring at progress above one half.
fn lit(shot: &mut Snapshot) -> Vec<RingProbe> {
    shot.settle(Duration::from_millis(500));
    shot.focus_rings()
        .into_iter()
        .filter(|probe| probe.progress > 0.5)
        .collect()
}

/// Presses the field in row `row` with the pointer.
fn press_row(shot: &mut Snapshot, row: usize) {
    shot.click(20.0, row as f32 * ROW + 10.0);
}

#[test]
fn a_pointer_press_focuses_a_field_without_a_ring() {
    let mut shot = mount();
    press_row(&mut shot, 0);
    assert!(lit(&mut shot).is_empty(), "a press draws no ring");
}

#[test]
fn a_tab_draws_the_ring_on_the_next_field() {
    let mut shot = mount();
    shot.key(Key::Named(NamedKey::Tab));
    let rings = lit(&mut shot);
    assert_eq!(rings.len(), 1, "{rings:?}");
    assert!(
        rings[0].face[1] < ROW,
        "the first field has the ring: {:?}",
        rings[0].face
    );

    shot.key(Key::Named(NamedKey::Tab));
    let rings = lit(&mut shot);
    assert_eq!(rings.len(), 1, "{rings:?}");
    assert!(
        rings[0].face[1] >= ROW,
        "the second field has the ring: {:?}",
        rings[0].face
    );
}

#[test]
fn a_press_after_a_tab_takes_the_ring_away() {
    let mut shot = mount();
    shot.key(Key::Named(NamedKey::Tab));
    assert_eq!(lit(&mut shot).len(), 1, "the Tab draws a ring");

    press_row(&mut shot, 1);
    assert!(
        lit(&mut shot).is_empty(),
        "a press after a Tab draws no ring"
    );
}
