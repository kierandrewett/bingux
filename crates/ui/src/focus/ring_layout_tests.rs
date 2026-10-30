//! Headless layout tests: every kind of face a control can build has its ring
//! around the painted box.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::focus::ring_check::{Mismatch, RingProbe, check};
use crate::motion::Focusable;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

/// The faces under test, each focused.
#[derive(PartialEq)]
struct Faces;

impl Component for Faces {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let text = |text: &str| label().text(text.to_string());
        rect()
            .tw(&tokens, &[flex_col(), p(S5), gap(S5)])
            .child(
                // A fixed-size face with uneven padding: the dropdown.
                rect()
                    .component("PaddedFixed")
                    .tw(
                        &tokens,
                        &[
                            w(Dim::DropdownMenu),
                            h(Dim::Control),
                            pl(S3),
                            pr(S2),
                            bg(Paint::Control),
                            rounded(Radius::Control),
                        ],
                    )
                    .child(text("Collabora Office")).focusable(Focus::Keyboard),
            )
            .child(
                // A face whose size comes from its padded content.
                rect()
                    .component("PaddedContent")
                    .tw(
                        &tokens,
                        &[py(S2), pl(S3), pr(S4), bg(Paint::Control), rounded(Radius::Control)],
                    )
                    .child(text("Content sized")).focusable(Focus::Keyboard),
            )
            .child(
                // A face that fills its row's width by flex while its
                // wrapper is sized by content.
                rect().tw(&tokens, &[w(Dim::Dialog), flex_row()]).child(
                    rect()
                        .component("FlexFace")
                        .tw(
                            &tokens,
                            &[
                                w_flex(),
                                h(Dim::Control),
                                px(S3),
                                bg(Paint::Control),
                                rounded(Radius::Control),
                            ],
                        )
                        .child(text("Flex face")).focusable(Focus::Keyboard),
                ),
            )
            .child(
                // A control with a margin around its wrapper.
                rect().tw(&tokens, &[m(S2)]).child(
                    rect()
                        .component("MarginFace")
                        .tw(
                            &tokens,
                            &[
                                w(Dim::DropdownMenu),
                                h(Dim::Control),
                                bg(Paint::Control),
                                rounded(Radius::Control),
                            ],
                        )
                        .child(text("Margin")).focusable(Focus::Keyboard),
                ),
            )
            .child(
                // A face under a parent that offsets its children.
                rect().offset_y(3.0).child(
                    rect()
                        .component("OffsetParent")
                        .tw(
                            &tokens,
                            &[
                                w(Dim::DropdownMenu),
                                h(Dim::Control),
                                bg(Paint::Control),
                                rounded(Radius::Control),
                            ],
                        ).focusable(Focus::Keyboard),
                ),
            )
    }
}

fn probes() -> Vec<RingProbe> {
    let mut shot = Snapshot::new(|| Faces.into_element(), (480.0, 640.0));
    shot.settle(std::time::Duration::from_millis(500));
    shot.focus_rings()
}

#[test]
fn every_kind_of_face_has_its_ring_around_the_painted_box() {
    let probes = probes();
    for probe in &probes {
        eprintln!("{probe:?}");
    }
    assert!(probes.len() >= 5, "found {} rings", probes.len());
    let found: Vec<Mismatch> = check(&probes);
    for mismatch in &found {
        eprintln!("MISMATCH {mismatch}");
    }
    assert!(found.is_empty(), "{} rings are off their face", found.len());
}
