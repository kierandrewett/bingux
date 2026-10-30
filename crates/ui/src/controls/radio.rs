//! Radio buttons: the 18 px circle that a radio row draws on its leading edge
//! (libadwaita's radio check buttons, SwiftUI's radio group picker). The row
//! takes focus and presses; the circle only paints.

use freya::animation::*;
use freya::prelude::*;

use crate::motion::use_tween_with;
use crate::style::*;
use crate::surface::use_tokens;

/// The option `count` options after `from` in the direction `forward`,
/// skipping options that `enabled` rejects and wrapping at the ends, as a
/// native radio group does. `None` when no other option is enabled.
pub(crate) fn next_enabled(
    count: usize,
    from: usize,
    forward: bool,
    enabled: impl Fn(usize) -> bool,
) -> Option<usize> {
    (1..count)
        .map(|step| {
            if forward {
                (from + step) % count
            } else {
                (from + count - step) % count
            }
        })
        .find(|&index| enabled(index))
}

/// The circle of a radio button: an accent disc with a white dot when
/// selected, an outline when not. The dot grows in as it is selected.
#[derive(Clone, PartialEq)]
pub(crate) struct RadioMark {
    pub selected: bool,
    pub hover: bool,
    pub press: bool,
}

impl Component for RadioMark {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let motion = tokens.motion(100);
        // The dot grows from nothing: a runtime size, so a raw `Size::px`.
        let target = if self.selected { Dim::Dot.px() } else { 0.0 };
        let dot = use_tween_with(target, target, motion, Function::Expo);
        let edge = if self.selected {
            border(Line::Ring(Paint::Accent))
        } else {
            border(Line::Outline)
        };
        let fill = if self.selected {
            bg(Paint::Accent)
        } else {
            surface(Paint::Control, self.hover, self.press, false)
        };
        rect()
            .tw(
                &tokens,
                &[
                    w(Dim::Check),
                    h(Dim::Check),
                    rounded(Radius::Pill),
                    center(),
                    fill,
                    edge,
                ],
            )
            .child(
                rect()
                    .width(Size::px(dot))
                    .height(Size::px(dot))
                    .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::OnAccent)]),
            )
    }
}
