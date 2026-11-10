//! A modal scope opened with an `autofocus` field takes the field as its first
//! focus. The trap's open effect moves focus to its start only when no control
//! inside claimed it, so typing on open reaches the field.

use std::time::Duration;

use accesskit::{NodeId, Role};
use freya::prelude::*;

use crate::controls::text_field::text_field;
use crate::focus::trap;
use crate::shell::overlays::OverlayHost;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;

#[derive(PartialEq)]
struct Modal {
    autofocus: bool,
}

impl Component for Modal {
    fn render(&self) -> impl IntoElement {
        provide_tokens();
        let value = use_state(String::new);
        let mut field = text_field(value, |_| {});
        if self.autofocus {
            field = field.autofocus();
        }
        OverlayHost::new(rect().expanded().child(trap(field)))
    }
}

/// The accessibility node of the one text input, and the node focus is on.
fn input_and_focus(autofocus: bool) -> (Option<NodeId>, Option<NodeId>) {
    let mut shot = Snapshot::new(move || Modal { autofocus }.into(), (400.0, 200.0));
    shot.settle(Duration::from_millis(300));
    let inputs: Vec<NodeId> = shot.runner().find_many(|_, element| {
        let data = element.accessibility();
        (data.builder.role() == Role::TextInput)
            .then(|| data.a11y_id)
            .flatten()
    });
    let focus = shot.runner().sync_and_update().focus;
    (inputs.first().copied(), Some(focus))
}

#[test]
fn an_autofocused_field_in_a_trap_keeps_the_first_focus() {
    let (input, focus) = input_and_focus(true);
    let input = input.expect("the field has an accessibility node");
    assert_eq!(
        focus,
        Some(input),
        "the field, not the trap's start, has focus"
    );
}

#[test]
fn a_trap_without_an_autofocus_field_still_focuses_its_start() {
    let (input, focus) = input_and_focus(false);
    let input = input.expect("the field has an accessibility node");
    assert_ne!(focus, Some(input), "no claim: the trap's start takes focus");
}
